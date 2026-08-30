# Problem Definition: Backtest Data-Leakage Auditor (`problem.md`)

> **Competition:** micro1 Frontier Engineering Challenge 2026  
> **Core Concept:** An agentic system that audits quantitative trading strategy backtests for look-ahead bias and data leakage — not by pattern-matching syntax, but by empirically re-running the strategy under a controlled perturbation and measuring whether performance actually depends on future information.

---

## 01. Target Persona: Who Has This Problem?

**Primary Users:**
- Independent quantitative researchers
- Retail algorithmic traders
- Engineers at boutique proprietary trading desks

### Context & Pain Point
These practitioners develop quantitative backtests before committing real financial capital to live execution. Unlike large institutional hedge funds, they work independently or in lean teams without dedicated quant-research peer review committees. Consequently, they serve as the single last line of defense against deploying corrupted, overfit, or leaky trading models to production.

---

## 02. The Core Bottleneck: Why Is It Worth Solving?

A backtest that displays stellar simulated returns is frequently built on information the strategy could never have accessed in real-time execution.

### Common Leakage Vectors
1. **Look-Ahead Bias:** An off-by-one error in a rolling window indicator or feature calculation.
2. **Execution Timing Mismatch:** Simulating order execution at the current bar's close price instead of the subsequent bar's open price.
3. **Improper Chronological Splits:** Inadvertently standardizing or splitting time-series data using future state (e.g., naive cross-validation or unaligned `df[:split_date]` splits).

> [!WARNING]
> **The Code-Review Trap:**  
> These bugs are notoriously subtle and practically invisible during manual code inspections: the syntax is valid, the business logic appears sound, and the backtest shows extraordinary Sharpe ratios — until capital is lost in live markets.

### Why Prior Art / Static Analysis Fails
- **Existing Tool:** [backtest-audit (GitHub)](https://github.com/mythofstars/backtest-audit)
- **Limitation:** A Python AST pattern-matcher containing six static rules.
- **Why It Falls Short:** By its own documentation, it **does not execute code**, cannot detect patterns beyond its static rulebook, and explicitly fails on manual chronological train/test splits (e.g., `df[:split_date]`) — the single most prevalent real-world time-series splitting technique.
- **Takeaway:** Static analysis alone cannot solve the empirical verification gap.

---

## 03. The Agentic Solution: How Does It Solve It Well?

The agent's defining design philosophy is **refusing to take its own diagnostic hunches on faith**.

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Reason & Hypothesize: Inspect source code for leakage    │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│ 2. Tool Execution (Shift-Test): Perturb features by +1 bar  │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│ 3. Empirical Verification: Rerun backtest & measure Sharpe  │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│ 4. Verdict & Recovery: Conclude or retry with new hypothesis│
└─────────────────────────────────────────────────────────────┘
```

### Diagnostic Mechanism (The Shift-Test)
1. **Form Hypothesis:** The agent inspects the strategy source code and identifies candidate leakage locations.
2. **Execute Shift-Test Perturbation:** The agent invokes a tool that shifts the strategy's input features forward by exactly one bar and triggers a full deterministic backtest rerun.
3. **Compare Empirical Metrics:**
   - **Clean Strategy:** Performance immediately collapses toward random noise under perturbation ($\Delta \text{Sharpe} \gg 0$).
   - **Leaky Strategy:** Performance remains suspiciously robust because the strategy is exploiting future-adjacent information.
4. **Evidence-Based Verdict:** The agent issues an audit verdict backed by quantifiable empirical delta measurements. If inconclusive, it explores an alternative hypothesis.

### Workflow Comparison

| Capability | Naive Baseline (`baseline.rs`) | Advanced Agentic Solution (`advanced.rs`) |
| :--- | :--- | :--- |
| **Audit Methodology** | Single-shot direct LLM prompt on raw source | Multi-step reasoning $\rightarrow$ Hypothesis $\rightarrow$ Tool use $\rightarrow$ Verification |
| **Empirical Grounding** | None (plausible-sounding educated guess) | Re-runs backtest under controlled feature perturbation |
| **Self-Correction** | ❌ No retry or falsification loop | ✅ Refines hypothesis if empirical data is ambiguous |
| **Output Deliverable** | Text claim with no validation | Verifiable report backed by empirical Sharpe delta ($\Delta$) |

### Challenge Evaluation Dimensions
- **Primary Metric:** Detection accuracy across a standardized test battery of seeded leaky strategies vs. clean controls.
- **Human Time Saved:** Compares the minutes a researcher spends manually instrumenting shift tests vs. automated agent execution.
- **Operational Cost:** LLM API token expense per audited strategy.

---

## 04. Reproducibility & Determinism

The entire evaluation harness is designed for complete, deterministic local reproducibility:

- **Seeded Synthetic Price Engine:** All market data is generated synthetically from fixed pseudo-random seeds.
- **Zero External Network Dependencies:** No reliance on external financial market APIs or paid data feeds.
- **Self-Contained Rust Engine:** Small, standalone Rust test cases compile and run via standard `cargo` commands with identical outputs on any machine.
- **Step-by-Step Guide:** Complete reproduction commands will be documented in [REPRODUCTION.md](REPRODUCTION.md).

---

## 05. Scope & Boundaries (v1 Build)

To maintain high engineering fidelity within the challenge timeline, scope is deliberately bounded:

### In-Scope (v1 Core)
- **3–4 Seeded Strategies:** Deliberate bug injections (wrong shift direction, wrong trade-timing bar, improper manual chronological split).
- **1–2 Clean Control Strategies:** Legitimate strategies that correctly collapse under perturbation.
- **Justified Empirical Threshold:** A defensible Sharpe-delta decision boundary.
- **Single Verification Tool:** Dedicated shift-test perturbation harness.

### Out-of-Scope (Stretch Goals / Iterations)
- Exhaustive suites of statistical significance frameworks (e.g., bootstrapped p-values).
- Arbitrary multi-asset portfolio engines or live market broker integrations.

---

## 06. Related Documentation

- **Agent Directives & Build Order:** [agents.md](agents.md)
- **Operating Guidelines:** [instructions.md](instructions.md)
- **Competition Framework:** micro1 Frontier Engineering Challenge 2026 (internal rules doc, not redistributed in this repository)
- **External Prior Art:** [backtest-audit Repository](https://github.com/mythofstars/backtest-audit)