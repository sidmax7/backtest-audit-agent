# Agent Directives & Implementation Roadmap (`agents.md`)

> [!IMPORTANT]
> **Read [problem.md](problem.md) first.** It answers who has this problem, why it matters, and what "solving it well" means. Do not start writing code until you have read it — the shape of the solution depends entirely on that framing.

---

## 1. Project Context & Scoring Rubric

This is a solo entry for the **micro1 Frontier Engineering Challenge 2026**.

### Scoring Breakdown
- **Agent Solution & Engineering:** 30% *(The single biggest factor — the design of the agent's verification loop matters more than raw code volume)*
- **End-to-End Quality:** 20%
- **Problem & User Value:** 15%
- **Measured Improvement:** 15%
- **Reproducibility:** 15%
- **Hot Take / Insights:** 5%

---

## 2. Architecture & Directory Layout

Single Rust crate (`frontier-challenge`).

> **Note (v1 locked):** the layout below is the *original build-order plan*, written before any code existed, and is kept as a record of the intended sequence. It is no longer an accurate description of the shipped crate — v1 added `static_checks.rs`, `llm/`, `audit_source.rs`, `verify/slippage.rs`, and `examples/inspect.rs`, and never built the optional `benches/`. For the as-built architecture, see the tree in [README.md](README.md) under "Architecture".

```text
frontier-challenge/
├── src/
│   ├── lib.rs              # Shared types and core logic
│   ├── engine/             # (To build) Synthetic price generator + backtest loop + metrics
│   ├── strategies/         # (To build) 3-4 seeded-bug strategies + 1-2 clean controls
│   ├── verify/             # (To build) The shift-test verifier
│   ├── telemetry/          # (To build) Trajectory recorder shared by both binaries — see §4
│   └── bin/
│       ├── baseline.rs     # One-shot LLM audit (no empirical verification loop)
│       └── advanced.rs     # Real agent: Hypothesize -> Call shift-test tool -> Verify -> Conclude
├── trajectories/            # (To build) One JSON trace per run, emitted by telemetry — see §4
├── tests/
│   └── acceptance.rs       # Runs evaluation across all seeded strategies, reports accuracy
└── benches/
    └── comparison.rs       # (Optional, low priority) Performance benchmarks
```

---

## 3. Build Order & Implementation Sequence

Follow this sequence strictly — **do not jump ahead**:

1. **Synthetic OHLCV Generator & Backtest Loop**  
   Build a seeded, deterministic synthetic OHLCV price generator and a minimal backtest loop (`position` in $\rightarrow$ `returns` and `Sharpe ratio` out). No agent, no bugs yet — just prove the engine runs correctly.

2. **Seeded Strategies & Clean Controls**  
   Seed exactly 3–4 strategies, each with one deliberate bug:
   - Wrong shift direction
   - Wrong trade-timing bar
   - Bad manual chronological split  
   Plus 1–2 clean control strategies.

3. **Shift-Test Verifier**  
   Implement the shift-test verifier: shift feature inputs forward one bar, rerun the backtest, compare Sharpe ratios, and apply a simple, justified threshold.

4. **Baseline Implementation (`baseline.rs`)**  
   One LLM call on raw source code with no empirical verification, recording its verdict.

5. **Advanced Agent Loop (`advanced.rs`)**  
   The real agent loop: reasoning $\rightarrow$ hypothesis generation $\rightarrow$ tool call to shift-test verifier $\rightarrow$ empirical verification $\rightarrow$ final conclusion (with retry against a second hypothesis if unresolved). Every step of this loop is recorded live to a trajectory file as it happens — see §4.

6. **Evaluation Harness & Benchmark Suite**  
   Run the evaluation harness across all seeded strategies to produce the **Measured Improvement** evidence.

7. **Documentation & Telemetry**  
   Maintain `README.md`, `CHANGELOG.md`, and the `trajectories/` logs (§4) iteratively throughout development, not as an afterthought.

---

## 4. Agent Trajectory Logging (Deliverable 4)

Per `instructions.md` Deliverable 4, every run of either binary must leave behind a clean, structured trace: **User Input → Agent Thought → Tool Call → Tool Output → Verification Step → Final Output**. This is graded evidence, not incidental logging — it's also how anyone picking up this repo later can confirm a claimed verdict actually came from the tool-use loop rather than a plausible-sounding summary written after the fact.

### Requirements
- Every invocation of `baseline` or `advanced` writes exactly one JSON file to `trajectories/`, named `<binary>_<strategy-name>_<unix-timestamp>.json`. A run is not complete until this file exists on disk.
- Implement this once as a shared `src/telemetry/` module (a `Trajectory` struct + `TrajectoryStep` enum) used by both binaries. Do not hand-roll separate ad hoc logging per binary.
- Steps are appended in order as they actually happen during execution, not reconstructed afterward — the file must be an honest trace, not a post-hoc summary.

### Schema (minimum fields)
```json
{
  "run_id": "uuid",
  "binary": "baseline | advanced",
  "strategy": "strategy module name",
  "provider": "anthropic | gemini | none",
  "model": "e.g. gemini-3.5-flash-lite, or n/a for audit_csv (no LLM call)",
  "started_at": "ISO-8601 timestamp",
  "steps": [
    { "step": 1, "type": "reasoning",     "content": "..." },
    { "step": 2, "type": "tool_call",     "tool": "shift_test", "input": { "shift_bars": 1 } },
    { "step": 3, "type": "tool_output",   "output": { "sharpe_clean": 1.8, "sharpe_shifted": 1.7 } },
    { "step": 4, "type": "verification",  "content": "delta below threshold -> inconclusive, retrying with alternate hypothesis" },
    { "step": 5, "type": "final_verdict", "verdict": "LEAKY | CLEAN", "sharpe_delta": 0.1, "confidence": "..." }
  ],
  "tokens_used": { "input": 0, "output": 0 },
  "wall_clock_ms": 0,
  "finished_at": "ISO-8601 timestamp"
}
```

`provider`/`model` name which LLM actually produced the verdict (added so a trajectory file is self-certifying about which model was used, not just that tokens were spent); `finished_at` pairs with `started_at` as an independently checkable timestamp, harder to fake by hand than `wall_clock_ms` alone.
`baseline` trajectories will typically collapse to a single `reasoning` + `final_verdict` step — that sparsity is itself part of the baseline-vs-advanced evidence, so don't pad baseline traces to artificially resemble the advanced agent's loop.

### Why this matters here specifically
The entire pitch of the advanced agent is that its verdict is backed by an empirical tool call, not vibes (see `problem.md` §03). The trajectory log is the artifact that proves that claim to a grader — without it, "Multi-step reasoning → Hypothesis → Tool use → Verification" is just an unverified claim in a README.

---

## 5. Model Routing & Specialization

*(For human reference and session expectations)*

| Model Tier | Primary Responsibilities | Scope & Expectations |
| :--- | :--- | :--- |
| **Opus** *(Deep Reasoning)* | Architecture decisions, agent-loop design, tricky compiler & logic debugging | Design decisions, failure recovery, hypothesis loops, hard bugs |
| **Gemini** *(Fast Execution)* | Boilerplate, test scaffolding, repetitive fixes, benchmarks, documentation | Scaffolding, mechanical implementation, formatting, test suites |

---

## 6. Non-Negotiable Constraints

- **Trajectory Logging Is Mandatory, Not Post-Hoc:** Every binary run emits a structured trajectory JSON to `trajectories/` per §4 before it exits. Capture steps live as they occur — never reconstruct a trajectory from memory after the run has finished.
- **Deterministic Synthetic Data Only (v1 scope):** All data is synthetic, seeded, and deterministic. Never fetch real market data or make any network call beyond the LLM API itself. v2 (see README "Roadmap") plans to vendor one frozen historical OHLCV series to validate calibration against real price statistics — that is a checked-in, fetched-once file, not a live network dependency, and does not relax this constraint's actual intent (no live external data source at run time).
- **Surface Errors Transparently:** Never silently fix or hide errors. Surface them in output and in `CHANGELOG.md` — the trajectory record must demonstrate authentic problem-solving.
- **Keep Verification Simple:** Keep the shift-test threshold simple and justified, not a full statistical significance framework. (Explicitly out of scope for v1 — do not add bootstrapped p-values without prior approval).
- **Strict Strategy Scope:** Do not expand beyond 4 seeded bug patterns without prior discussion. Breadth is not the goal; a clean, rigorously evidenced narrow case is. (v1 shipped with 8 registered strategies — every expansion beyond the original 4 happened via explicit discussion in-session, consistent with this constraint's actual intent, not in spite of it.)
- **Iterative Changelog Tracking:** Every meaningful change gets a [CHANGELOG.md](CHANGELOG.md) entry detailing what changed and what empirical evidence (test result, metric, failure) motivated it.
- **Milestone Tagging:** Commit and tag at each milestone matching changelog entries, ensuring every writeup claim traces back to a verified commit.

---

## 7. Definition of Done (v1) — LOCKED

v1 is locked as of `git tag v1`. All items below are complete; no further v1 scope changes should land on top of this without explicitly reopening it. Future work is v2 scope — see README.md "Roadmap: v1 (locked) and v2" for the planned direction (real-market-data calibration validation).

- [x] `cargo run --bin baseline -- <strategy-file>` prints a verdict with no empirical check behind it.
- [x] `cargo run --bin advanced -- <strategy-file>` prints a verdict backed by an actual shift-test rerun, displaying the Sharpe delta ($\Delta$).
- [x] `cargo test` runs the full evaluation across all seeded strategies and reports detection accuracy for both baseline and advanced agent.
- [x] `trajectories/` contains a JSON trace (per the §4 schema) for every seeded strategy, for both `baseline` and `advanced` runs.
- [x] [README.md](README.md) and [CHANGELOG.md](CHANGELOG.md) are fully up to date and substantive before declaring completion.
- [x] [VIDEO_SCRIPT.md](VIDEO_SCRIPT.md) (Deliverable 3, `instructions.md` §6) is a timed, shootable outline covering Problem → Baseline Failure → Agent Solution → Benchmark Proof → What Was Removed, sourced from real captured evidence (trajectories, CHANGELOG numbers) rather than invented B-roll.

All four `instructions.md` §6 submission deliverables are complete as of this line.

---

## 8. References & Prior Art

- **Problem Framing:** [problem.md](problem.md)
- **Prior Art / Competitive Differentiation:** [backtest-audit (GitHub)](https://github.com/mythofstars/backtest-audit)  
  *Context:* Python AST static analysis with hardcoded rules. Does not execute code, cannot detect bugs outside its fixed list, and explicitly misses manual chronological splits (e.g., `df[:split_date]`).