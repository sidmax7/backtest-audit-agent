//! The real agent: reasoning -> hypothesis (optionally grounded by a fast
//! static scan) -> empirical tool call(s) -> verification -> verdict, with
//! a targeted retry when the model's read of the evidence disagrees with
//! the empirically-justified rule -- see `problem.md` §03 step 4 ("If
//! inconclusive, it explores an alternative hypothesis").
//!
//! Three tools are available: `static_scan` (a fast, free, deterministic
//! AST pass, `static_checks::scan`), `shift_test` (the real empirical
//! backtest rerun on the temporal axis, `verify::shift_test`), and
//! `slippage_test` (the real empirical backtest rerun on the
//! execution-realism axis, `verify::slippage::slippage_test`). `static_scan`
//! is optional -- a hint for hypothesis formation, not proof either way.
//! `shift_test` and `slippage_test` are the two things a verdict actually
//! has to be grounded in; the loop below nudges toward both and ultimately
//! falls back to running whichever was skipped directly, so the reported
//! verdict is always empirically grounded on both axes regardless of what
//! the model chose to call.

use anyhow::{Context, Result};
use clap::Parser;
use frontier_challenge::audit_source::strip_comments;
use frontier_challenge::engine::{generate_series, PriceParams};
use frontier_challenge::llm::{Client, ContentBlock, Message, ToolDef};
use frontier_challenge::static_checks;
use frontier_challenge::strategies::find_by_name;
use frontier_challenge::telemetry::Trajectory;
use frontier_challenge::verify::{
    audit, audit_slippage, combine_signals, localize, shift_test, slippage_test, AgentVerdict,
    ShiftTestResult, SlippageTestResult, Verdict,
};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    /// Path to a strategy source file, e.g.
    /// src/strategies/forward_window_lookahead.rs
    strategy_file: PathBuf,
}

const MAX_TURNS: usize = 4;

const SYSTEM_PROMPT: &str = "You are auditing a quantitative trading strategy's Rust source code for data leakage and look-ahead bias. Leakage comes in more than one shape: features computed from future bars or statistics fit on the whole series instead of only past data (a temporal axis), but also an unrealistic assumption about what price you'd actually get filled at baked into the PnL calculation itself (an execution-realism axis) -- a strategy can be completely honest on one axis and leaky on the other. \
You have three tools. static_scan is a fast, free, deterministic static-analysis pass over the source (no LLM, no backtest) -- useful for forming or corroborating a hypothesis quickly, but it only catches specific known temporal patterns, so no findings does not by itself prove the strategy is clean on any axis. shift_test is the real empirical check on the temporal axis: it reruns the actual backtest with the strategy's computed features shifted forward by exactly one bar and reports the Sharpe ratio before and after. slippage_test is the real empirical check on the execution-realism axis: it reruns the actual backtest with the strategy's own declared fill-price assumption versus the honest default, and reports the Sharpe delta between them -- a strategy can be perfectly robust under shift_test and still be leaky here, because shift_test only ever perturbs the feature, never the fill assumption. Your final verdict must be grounded in shift_test AND slippage_test, not in static_scan alone, and not in shift_test alone. \
An honest strategy's temporal edge is fragile: shifting it degrades performance toward noise. A leaky strategy is often robust to the shift because the contaminating information isn't confined to one bar. Also weigh the ORIGINAL Sharpe on its own merits: this synthetic market has roughly 1% daily volatility and only a small genuine mean-reversion edge, so a Sharpe ratio far beyond what any real signal could sustain here is itself suspicious, even in a case where the shift happens to degrade it a lot. Separately, a large positive slippage_test delta means the strategy's own fill assumption -- not its feature -- is doing real, unearned work. \
Form a hypothesis (static_scan can help), call BOTH shift_test and slippage_test to empirically verify it -- a strategy that looks clean under one can still be leaky under the other -- then state your verdict as exactly one word on the first line of your final answer: LEAKY or CLEAN, followed by a short justification referencing the actual numbers returned. If static_scan flags a real pattern but both empirical tests show the strategy is robust, that is a genuine disagreement between your tools -- say so plainly (you may answer INCONCLUSIVE) rather than forcing a confident pick either way.";

fn shift_test_tool() -> ToolDef {
    ToolDef {
        name: "shift_test".to_string(),
        description: "Reruns the strategy's real backtest against the same deterministic synthetic price series twice: once as coded, once with its computed feature series shifted forward by exactly one bar (a stand-in for 'what if this had to rely on slightly older information'). Returns sharpe_original, sharpe_shifted, and their delta. When the strategy's feature is built from more than one named sub-computation, also returns a per-feature breakdown pointing at which specific component carries a leak, not just whether one exists somewhere in the combined feature. Takes no strategy identifier -- it always tests the strategy currently under audit.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "hypothesis": {
                    "type": "string",
                    "description": "The specific look-ahead mechanism you suspect in the source, stated before running the test."
                }
            },
            "required": ["hypothesis"]
        }),
    }
}

fn slippage_test_tool() -> ToolDef {
    ToolDef {
        name: "slippage_test".to_string(),
        description: "Reruns the strategy's real backtest on the execution-realism axis: once using the strategy's own declared fill-price assumption (Strategy::realized_return), once forced to the honest close-to-close default, using the same feature and offset both times. Returns sharpe_as_declared, sharpe_honest_fill, and their delta. A strategy can be completely robust under shift_test (no temporal look-ahead in its feature at all) and still be leaky here, because shift_test never perturbs the fill assumption -- only slippage_test does. Takes no strategy identifier -- it always tests the strategy currently under audit.".to_string(),
        input_schema: json!({"type": "object", "properties": {}, "required": []}),
    }
}

fn static_scan_tool() -> ToolDef {
    ToolDef {
        name: "static_scan".to_string(),
        description: "Parses the strategy's own source as a real Rust AST (no execution, no LLM) and checks for three known look-ahead patterns: a window/index bound that reaches forward past the current bar, a statistic computed over the whole series rather than a windowed sub-range, and an execution offset that doesn't wait for the next bar. Returns a list of findings, or none. Fast and free -- a good first move -- but narrow: absence of a finding does not prove the strategy is clean.".to_string(),
        input_schema: json!({"type": "object", "properties": {}, "required": []}),
    }
}

/// Names whichever of the two required empirical tools hasn't been called
/// yet, for nudge/fallback messages -- "shift_test", "slippage_test", or
/// "shift_test and slippage_test" when neither has.
fn missing_tools_label(
    empirical_result: &Option<ShiftTestResult>,
    slippage_result: &Option<SlippageTestResult>,
) -> &'static str {
    match (empirical_result.is_none(), slippage_result.is_none()) {
        (true, true) => "shift_test and slippage_test",
        (true, false) => "shift_test",
        (false, true) => "slippage_test",
        (false, false) => "",
    }
}

fn parse_verdict(text: &str) -> Option<AgentVerdict> {
    let first_word = text
        .lines()
        .next()?
        .trim()
        .trim_end_matches('.')
        .to_uppercase();
    if first_word.starts_with("LEAKY") {
        Some(AgentVerdict::Leaky)
    } else if first_word.starts_with("CLEAN") {
        Some(AgentVerdict::Clean)
    } else if first_word.starts_with("INCONCLUSIVE") {
        Some(AgentVerdict::Inconclusive)
    } else {
        None
    }
}

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
    let entry = find_by_name(&strategy_name).with_context(|| {
        format!(
            "'{strategy_name}' is not a registered seeded strategy (see strategies::all_strategies); \
             the advanced agent can only empirically verify strategies the harness knows how to rerun."
        )
    })?;

    let client = Client::from_env()?;
    let mut trajectory = Trajectory::new(
        "advanced",
        &strategy_name,
        client.provider_label(),
        client.model(),
    );
    let bars = generate_series(PriceParams::default());
    let tools = vec![shift_test_tool(), slippage_test_tool(), static_scan_tool()];

    // Comments are stripped, and the file path/name is never included in
    // the prompt: both would otherwise hand the LLM a spoiler (e.g. a doc
    // comment or a struct name literally admitting "leaky") that trivializes
    // "detection" and invalidates the whole baseline-vs-advanced comparison.
    // See CHANGELOG for how this was found.
    let sanitized_source = strip_comments(&source);

    let user_prompt = format!(
        "Strategy source:\n\n```rust\n{sanitized_source}\n```\n\nInspect this for look-ahead bias or data leakage, on both the temporal axis and the execution-realism axis. State your initial hypothesis, then call both shift_test and slippage_test to empirically verify it before concluding -- a strategy can be clean under one and leaky under the other."
    );
    trajectory.record_reasoning(format!(
        "Read a {}-byte strategy (comments stripped before showing the LLM); three tools are available (static_scan, shift_test, slippage_test).",
        sanitized_source.len()
    ));

    let mut messages = vec![Message::user_text(&user_prompt)];
    let mut empirical_result: Option<ShiftTestResult> = None;
    let mut slippage_result: Option<SlippageTestResult> = None;
    let mut static_findings_present = false;

    let mut response = client.send(SYSTEM_PROMPT, &messages, &tools)?;
    trajectory.add_tokens(response.input_tokens, response.output_tokens);

    // The API allows a single turn to contain multiple tool_use blocks
    // (parallel tool use), so each round executes *all* calls the model
    // made and returns all their results in one message, not just the
    // first. Bounded to MAX_TURNS round-trips; a model that keeps calling
    // static_scan without ever reaching for shift_test still gets a
    // shift_test result -- either via a nudge, or the harness running it
    // directly as a last resort -- because that's the one thing the
    // reported verdict actually has to be grounded in.
    for turn in 1..=MAX_TURNS {
        let text = response.text();
        if !text.trim().is_empty() {
            trajectory.record_reasoning(text);
        }

        let calls: Vec<(String, String, Value)> = response
            .tool_uses()
            .iter()
            .map(|(id, name, input)| (id.to_string(), name.to_string(), (*input).clone()))
            .collect();

        if calls.is_empty() {
            if empirical_result.is_some() && slippage_result.is_some() {
                break; // final answer received after both empirical tools were used
            }
            if turn == MAX_TURNS {
                trajectory.record_reasoning(format!(
                    "Model never called {}. Falling back: the harness runs {} directly so the \
                     reported verdict is still empirically grounded on both axes, and this \
                     failure mode is surfaced rather than hidden.",
                    missing_tools_label(&empirical_result, &slippage_result),
                    if empirical_result.is_none() && slippage_result.is_none() {
                        "them"
                    } else {
                        "it"
                    }
                ));
                if empirical_result.is_none() {
                    empirical_result = Some(shift_test(&bars, entry.strategy.as_ref()));
                }
                if slippage_result.is_none() {
                    slippage_result = Some(slippage_test(&bars, entry.strategy.as_ref()));
                }
                break;
            }
            trajectory.record_reasoning(format!(
                "Model made no tool call this turn; nudging it toward {}.",
                missing_tools_label(&empirical_result, &slippage_result)
            ));
            messages.push(Message::user_text(format!(
                "You have not called {} yet. Call it before concluding -- an unverified read is not an audit.",
                missing_tools_label(&empirical_result, &slippage_result)
            )));
            response = client.send(SYSTEM_PROMPT, &messages, &tools)?;
            trajectory.add_tokens(response.input_tokens, response.output_tokens);
            continue;
        }

        messages.push(Message {
            role: "assistant",
            content: response.content,
        });

        let mut result_blocks = Vec::with_capacity(calls.len());
        for (id, name, input) in &calls {
            let content = match name.as_str() {
                "shift_test" => {
                    let hypothesis = input
                        .get("hypothesis")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(no hypothesis stated)")
                        .to_string();
                    trajectory.record_tool_call(
                        "shift_test",
                        json!({"strategy": strategy_name, "hypothesis": hypothesis}),
                    );
                    let result = shift_test(&bars, entry.strategy.as_ref());
                    let components = localize(&bars, entry.strategy.as_ref());
                    trajectory.record_tool_output(json!({
                        "sharpe_original": result.sharpe_original,
                        "sharpe_shifted": result.sharpe_shifted,
                        "delta": result.delta,
                        "components": components.iter().map(|(name, r, v)| json!({
                            "name": name, "delta": r.delta, "verdict": format!("{v:?}"),
                        })).collect::<Vec<_>>(),
                    }));
                    let mut text = format!(
                        "sharpe_original={:.3} sharpe_shifted={:.3} delta={:.3}",
                        result.sharpe_original, result.sharpe_shifted, result.delta
                    );
                    // Localization: when this strategy's feature is built
                    // from more than one named component, shift-test each
                    // independently so the report can point at which one
                    // actually carries the leak, not just that one exists
                    // somewhere in the combined feature.
                    if components.len() > 1 {
                        text.push_str("\n\nper-feature breakdown:");
                        for (name, r, verdict) in &components {
                            text.push_str(&format!(
                                "\n- {name}: delta={:.3} -> {verdict:?}",
                                r.delta
                            ));
                        }
                    }
                    empirical_result = Some(result);
                    text
                }
                "slippage_test" => {
                    trajectory
                        .record_tool_call("slippage_test", json!({"strategy": strategy_name}));
                    let result = slippage_test(&bars, entry.strategy.as_ref());
                    trajectory.record_tool_output(json!({
                        "sharpe_as_declared": result.sharpe_as_declared,
                        "sharpe_honest_fill": result.sharpe_honest_fill,
                        "delta": result.delta,
                    }));
                    let text = format!(
                        "sharpe_as_declared={:.3} sharpe_honest_fill={:.3} delta={:.3}",
                        result.sharpe_as_declared, result.sharpe_honest_fill, result.delta
                    );
                    slippage_result = Some(result);
                    text
                }
                "static_scan" => {
                    trajectory.record_tool_call("static_scan", json!({"strategy": strategy_name}));
                    let findings = static_checks::scan(&sanitized_source).unwrap_or_default();
                    trajectory.record_tool_output(json!({
                        "findings": findings.iter().map(|f| json!({"rule": f.rule, "message": f.message})).collect::<Vec<_>>(),
                    }));
                    static_findings_present = static_findings_present || !findings.is_empty();
                    if findings.is_empty() {
                        "no static findings (this alone does not prove the strategy clean -- \
                         these are narrow, specific patterns, not a general leakage prover)"
                            .to_string()
                    } else {
                        findings
                            .iter()
                            .map(|f| format!("[{}] {}", f.rule, f.message))
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                }
                other => format!("unknown tool '{other}'"),
            };
            result_blocks.push(ContentBlock::ToolResult {
                tool_use_id: id.clone(),
                content,
            });
        }
        messages.push(Message {
            role: "user",
            content: result_blocks,
        });

        response = client.send(SYSTEM_PROMPT, &messages, &tools)?;
        trajectory.add_tokens(response.input_tokens, response.output_tokens);

        if turn == MAX_TURNS {
            if empirical_result.is_none() {
                empirical_result = Some(shift_test(&bars, entry.strategy.as_ref()));
            }
            if slippage_result.is_none() {
                slippage_result = Some(slippage_test(&bars, entry.strategy.as_ref()));
            }
        }
    }

    let empirical_result = empirical_result.context("no shift-test result available")?;
    let slippage_result = slippage_result.context("no slippage-test result available")?;
    let temporal_verdict = audit(&empirical_result);
    let slippage_verdict = audit_slippage(&slippage_result);
    trajectory.record_verification(format!(
        "shift_test: sharpe_original={:.3} sharpe_shifted={:.3} delta={:.3} -> two-signal rule says {:?}",
        empirical_result.sharpe_original,
        empirical_result.sharpe_shifted,
        empirical_result.delta,
        temporal_verdict
    ));
    trajectory.record_verification(format!(
        "slippage_test: sharpe_as_declared={:.3} sharpe_honest_fill={:.3} delta={:.3} -> {:?}",
        slippage_result.sharpe_as_declared,
        slippage_result.sharpe_honest_fill,
        slippage_result.delta,
        slippage_verdict
    ));

    // Two independent ways to falsify a leakage hypothesis, not a
    // primary/secondary pair: either tool reporting Leaky makes the
    // strategy empirically Leaky, regardless of what the other says.
    let effective_empirical = if slippage_verdict == Verdict::Leaky {
        Verdict::Leaky
    } else {
        temporal_verdict
    };

    // What the tools together imply the agent should report -- Leaky or
    // Clean when static agrees (or static_scan found nothing, its normal
    // silent case), Inconclusive on a genuine conflict: static_scan flagged
    // something but both empirical tools came back Clean anyway.
    let signal_verdict = combine_signals(static_findings_present, effective_empirical);
    if signal_verdict == AgentVerdict::Inconclusive {
        trajectory.record_verification(format!(
            "static_scan flagged a pattern but both empirical tools say Clean -- signals disagree, reporting Inconclusive (static_findings_present={static_findings_present})"
        ));
    }

    let mut final_text = response.text();
    let mut model_verdict = parse_verdict(&final_text);

    // Self-correction: if the model's read disagrees with what the signals
    // together imply, give it one more targeted look before finalizing.
    // Skipped when the signals themselves are Inconclusive -- there is
    // nothing to nudge the model toward matching in that case.
    if signal_verdict != AgentVerdict::Inconclusive && model_verdict != Some(signal_verdict) {
        trajectory.record_reasoning(format!(
            "Model's verdict ({model_verdict:?}) disagrees with the combined empirical rule ({effective_empirical:?}, temporal={temporal_verdict:?}, slippage={slippage_verdict:?}). Offering one alternative-hypothesis pass."
        ));
        messages.push(Message {
            role: "assistant",
            content: vec![ContentBlock::Text {
                text: final_text.clone(),
            }],
        });
        messages.push(Message::user_text(
            "Reconsider on both axes: don't judge temporal robustness by the shift_test delta alone -- is sharpe_original, taken on its own, plausible for a legitimate signal on ~1% daily volatility synthetic data with only a small real mean-reversion edge? And separately, does slippage_test's delta show the strategy's own declared fill assumption doing real, unearned work? State your final verdict as the first line: LEAKY or CLEAN.",
        ));
        let retry = client.send(SYSTEM_PROMPT, &messages, &tools)?;
        trajectory.add_tokens(retry.input_tokens, retry.output_tokens);
        final_text = retry.text();
        model_verdict = parse_verdict(&final_text);
        trajectory.record_verification(format!("alternate-hypothesis pass -> {model_verdict:?}"));
    }

    let (reported_verdict, confidence) = if signal_verdict == AgentVerdict::Inconclusive {
        (
            AgentVerdict::Inconclusive,
            format!(
                "inconclusive: static_scan flagged a pattern but both empirical tools \
                 (shift_test delta={:.3}, slippage_test delta={:.3}) found the strategy robust \
                 -- signals disagree, not confidently resolved either way",
                empirical_result.delta, slippage_result.delta
            ),
        )
    } else {
        let verdict = model_verdict.unwrap_or(signal_verdict);
        let confidence = if model_verdict == Some(signal_verdict) {
            "model agrees with the combined empirical rule".to_string()
        } else if model_verdict.is_none() {
            "model gave no parseable verdict; fell back to the combined empirical rule".to_string()
        } else {
            "model disagrees with the combined empirical rule after retry; reporting model's stated verdict"
                .to_string()
        };
        (verdict, confidence)
    };
    trajectory.record_final_verdict(
        format!("{reported_verdict:?}").to_uppercase(),
        empirical_result.delta,
        confidence,
    );

    println!("strategy: {strategy_name}");
    println!(
        "shift-test:    sharpe_original={:.3} sharpe_shifted={:.3} delta={:.3} -> {:?}",
        empirical_result.sharpe_original,
        empirical_result.sharpe_shifted,
        empirical_result.delta,
        temporal_verdict
    );
    println!(
        "slippage-test: sharpe_as_declared={:.3} sharpe_honest_fill={:.3} delta={:.3} -> {:?}",
        slippage_result.sharpe_as_declared,
        slippage_result.sharpe_honest_fill,
        slippage_result.delta,
        slippage_verdict
    );
    println!("agent verdict: {reported_verdict:?}");
    // Machine-parseable line for tests/acceptance.rs; kept separate from the
    // human-readable line above so reformatting that one can't break parsing.
    println!(
        "VERDICT: {}",
        match reported_verdict {
            AgentVerdict::Leaky => "LEAKY",
            AgentVerdict::Clean => "CLEAN",
            AgentVerdict::Inconclusive => "INCONCLUSIVE",
        }
    );
    println!("\n--- model's final answer ---\n{final_text}");

    let trajectory_dir = PathBuf::from("trajectories");
    let path = trajectory.finish_and_write(&trajectory_dir)?;
    eprintln!("\ntrajectory written to {}", path.display());

    Ok(())
}
