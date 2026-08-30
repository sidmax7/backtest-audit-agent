# Backtest Data-Leakage Auditor

An agentic system that audits quantitative trading strategy backtests for look-ahead bias and data leakage — not by pattern-matching syntax, but by empirically re-running the strategy under a controlled perturbation and measuring whether its performance actually depends on future information.

Built for the micro1 Frontier Engineering Challenge 2026. Full problem framing: [problem.md](problem.md). Reproduction guide: [REPRODUCTION.md](REPRODUCTION.md). Iteration history: [CHANGELOG.md](CHANGELOG.md).

Licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.

## Who this is for, and the bottleneck it addresses

```
Backtest on historical data
  Sharpe ratio looks excellent
   │
   │  "looks proven -- ship it"
   ▼
Paper trading
  same strategy, real market data, fake money
   │
   ▼
Edge quietly vanishes
   │
   ▼
The backtest had been using information it could
never have had in real time -- a leak, not a real
edge. That's the gap this project closes: catch
the leak before paper trading, not after.
```
*(This project doesn't run paper trading itself — it exists to catch the leak in the backtest before you ever get there.)*

Independent quantitative researchers, retail algorithmic traders, and engineers at boutique prop desks build backtests before risking real capital, without a dedicated quant-research peer review committee behind them. A backtest with a stellar Sharpe ratio is frequently built on information the strategy could never have accessed in real-time execution — an off-by-one error in a rolling window, execution simulated at the wrong bar's price, a chronological split standardized on the whole series. These bugs are syntactically valid and read as sound code; nothing about manual review catches them. [backtest-audit](https://github.com/mythofstars/backtest-audit), the existing static-analysis prior art, is explicit about its own limitation: it doesn't execute code, and it misses the single most common real-world leak (`df[:split_date]`-style manual splits) by its own documentation.

## Why this needs to be agentic, not a single LLM call

A one-shot prompt asking "is this leaky?" is a plausible-sounding guess with no way to falsify itself. The agent here instead runs a fixed loop:

```
1. Reason & Hypothesize   -- inspect the strategy's source (optionally grounded by a
                              fast static_scan of the same source), name a suspected
                              mechanism
2. Tool Execution         -- call shift_test (shift the features forward one bar) and
                              slippage_test (swap the declared fill assumption for an
                              honest one), rerunning the real backtest each time
3. Empirical Verification -- compare Sharpe before/after on both axes against justified
                              thresholds
4. Verdict & Recovery     -- conclude, retry with an alternative hypothesis if the first
                              read disagrees with the evidence, or report INCONCLUSIVE if
                              the tools genuinely conflict
```

Three tools, not one: `static_scan` is a fast, free, deterministic Rust-AST pass (`src/static_checks.rs`) for corroborating a hypothesis quickly; `shift_test` and `slippage_test` are the two empirical checks a verdict actually has to be grounded in, perturbing *when* a feature is used and *what fill price* the backtest assumes respectively. Static analysis and empirical verification catching the same bugs from different angles is the point — see "Static analysis, done properly" and "A second leakage axis" below.

That loop is `src/bin/advanced.rs`. `src/bin/baseline.rs` is the naive comparison point: one LLM call on raw source, no tool, no empirical check. Both binaries strip comments and never show the file path before the source reaches the LLM (`src/audit_source.rs`) — the strategy files' own doc comments and struct names admit their verdict outright, so without stripping them "detection" would just be reading a label. See CHANGELOG Iteration 5 for how that was found.

## Architecture

```
frontier-challenge/
├── src/
│   ├── engine/       synthetic seeded OHLCV generator + backtest loop (feature -> Sharpe)
│   ├── strategies/   6 leaky strategies (one per problem.md leakage category,
│   │                 plus three proving out extensibility, leak localization,
│   │                 and a second leakage axis) + 2 honest controls, one file
│   │                 each -- these ARE the source files the agent reads
│   ├── verify/       the shift-test tool + its two-signal audit rule, and
│   │                 slippage.rs -- the execution-realism (fill-price) axis
│   ├── static_checks.rs  the static_scan tool -- 3 rules over a real syn-parsed AST
│   ├── telemetry/    Trajectory recorder shared by both binaries (see below)
│   ├── llm/          provider-agnostic LLM client (Anthropic + Gemini backends)
│   ├── audit_source.rs  strips comments before source reaches the LLM (see below)
│   ├── csv_input.rs  adapter: audit a backtest from any language, via CSV
│   └── bin/
│       ├── baseline.rs   one-shot LLM audit, no empirical loop
│       ├── advanced.rs   the real agent loop described above
│       └── audit_csv.rs  deterministic shift-test on user-supplied CSV (no LLM)
├── tests/acceptance.rs   runs both binaries across every seeded strategy, reports accuracy
├── examples/inspect.rs   prints the raw shift-test numbers per strategy, across 4 seeds
└── trajectories/         one JSON trace per binary run (see below)
```

## The shift-test, and why it's two signals, not one

`verify::shift_test` reruns a strategy's backtest with its computed feature series shifted forward one bar. An honest strategy's edge degrades toward noise under that perturbation; a strategy leaking *persistent* future information (a forward-looking rolling window, or statistics fit on the whole series) stays robust, because the contamination isn't confined to one bar.

That single perturbation has a structural blind spot, discovered empirically while building this (see CHANGELOG Iteration 2): a bug that is itself an *exactly-one-bar* offset error (executing against the same bar's return instead of the next bar's) gets mathematically repaired, not exposed, by a one-bar shift. `verify::audit` therefore combines two signals — the shift delta, and an implausible-Sharpe ceiling calibrated against what the honest controls actually achieve — so the offset bug is still caught, just via the second signal instead of the first:

| strategy | leakage category (problem.md) | sharpe (original) | sharpe (shifted) | delta | verdict |
|---|---|---|---|---|---|
| `forward_window_lookahead` | 1: off-by-one rolling window | 9.42 | 6.70 | 2.72 | **Leaky** (ceiling) |
| `global_normalization_leak` | 3: improper chronological split | -0.23 | -0.26 | 0.03 | **Leaky** (robust to shift) |
| `execution_timing_mismatch` | 2: execution timing mismatch | 9.41 | -0.93 | 10.35 | **Leaky** (ceiling — the shift-test's own blind spot) |
| `forward_volatility_breakout` | 1: off-by-one rolling window (volatility variant, added post-hoc — see REPRODUCTION.md §8) | 0.32 | 0.55 | -0.24 | **Leaky** (robust to shift) |
| `mixed_signal_reversion` | 1: off-by-one rolling window, mixed with an honest component (see "Leak localization" below) | 3.78 | 0.91 | 2.87 | **Leaky** (ceiling) |
| `honest_mean_reversion` | (clean control) | 1.74 | -0.53 | 2.27 | **Clean** |
| `honest_trailing_window` | (clean control) | 1.75 | -0.53 | 2.27 | **Clean** |

7/7 correct, reproduced across 4 independent seeds (28/28), locked in as a regression test (`verify::tests::shift_test_correctly_classifies_every_seeded_strategy_across_seeds`) that runs in plain `cargo test` — no LLM or network call required, since this part of the evidence is fully deterministic. Full numbers and how the thresholds were picked: [CHANGELOG.md](CHANGELOG.md). For how trivial it was to add the 6th strategy, and what "showing it working" actually meant here, see [REPRODUCTION.md §8](REPRODUCTION.md#8-extending-this-adding-a-6th-strategy).

## Static analysis, done properly

`problem.md` differentiates this project from static analysis (see "Prior art, for real" below) — but that's an argument against a *specific* narrow rulebook, not against the technique. `src/static_checks.rs` is a real Rust-AST pass (via `syn`, not string matching) with three rules built for this codebase's actual bug shapes: `SA-EXEC-OFFSET` (a non-honest `execution_offset()`), `SA-FORWARD-WINDOW` (a window bound reaching forward, `i + ...`), `SA-GLOBAL-STAT` (`.iter().sum()` over the whole series, not a slice). Offline, zero LLM cost, **7/7 correct with zero false positives** (`static_checks::tests::scan_correctly_classifies_every_seeded_strategy`, runs in plain `cargo test`).

`advanced` exposes this as a second tool, `static_scan`, alongside `shift_test` — optional, fast, a good first move for forming a hypothesis, but not sufficient on its own (no findings doesn't prove a strategy clean). Static analysis and empirical verification are complementary signals here, not competing philosophies: the live trajectory for `execution_timing_mismatch` after this tool was added shows the model calling `static_scan` first, getting `SA-EXEC-OFFSET` back, forming a hypothesis that explicitly cites it, then confirming with `shift_test` — and reaching the correct verdict on the first pass, no self-correction retry needed (compare to the same strategy's Iteration 5 trajectory, which needed one). See CHANGELOG Iteration 7.

## Uncertainty reporting: when the tools disagree

Before this, `advanced` forced a binary verdict even when its two tools genuinely conflicted: if `static_scan` flagged something but `shift_test`'s empirical rule came back Clean anyway, the self-correction retry (above) would just keep nudging the model toward the empirical side until it complied. That silently threw away a real signal -- a static false positive and a genuine empirical blind spot look identical from the outside once the disagreement is papered over.

`verify::combine_signals(static_findings_present, empirical_verdict)` makes the disagreement explicit instead: static silence is never a signal either way (the layer is documented as narrow), but static flagging something *while* the empirical rule says Clean is a real conflict, reported as a third state, `Inconclusive`, with a confidence string naming exactly what disagreed -- not picked for the model, and not silently resolved by another retry. On the existing battery this is a pure no-op (every strategy where static ever fires is also empirically Leaky, so `combine_signals` reduces to today's exact behavior); it's proven with a dedicated fixture instead -- real source text with a harmless `let idx = i + 0;` no-op that trips a genuine `SA-FORWARD-WINDOW` false positive, paired with the actual honest logic (mechanically identical to `PriorReturnFade`) running through the real `shift_test`/`audit` and coming back Clean (`verify::tests::combine_signals_reports_inconclusive_on_a_genuine_static_vs_empirical_disagreement`).

## Leak localization: pointing at the carrier, not just confirming one exists

`shift_test` used to answer one question -- is this strategy's *whole* feature leaky -- with no way to say which part of it, if the feature was built from more than one ingredient. `Strategy::feature_components` lets a strategy expose its real sub-computations (defaulting to a single component identical to the whole feature, a true no-op for every strategy that doesn't override it), and `verify::localize` shift-tests each one independently, reusing `audit`'s existing calibrated thresholds directly -- no new calibration needed, since a component is just a feature vector like any other.

The concrete proof this actually discriminates, not just confirms uniformly: `mixed_signal_reversion.rs` sums an honest prior-return component with a small forward-window one. As a whole it's genuinely Leaky (`sharpe_original=3.784`, `delta=2.872` — caught by the implausible-Sharpe ceiling; the honest half of the sum keeps it fragile enough under shift that the delta signal alone would have missed it) -- but `localize` separates the two and verdicts them differently:

| component | delta | verdict |
|---|---|---|
| `prior_return_component` | 2.27 | **Clean** |
| `forward_window_component` | 2.72 | **Leaky** |

Wired into the `shift_test` tool itself (a per-feature breakdown is appended to its result text whenever a strategy has more than one component, byte-identical output otherwise), a real live run against this strategy shows `advanced` citing the breakdown directly in its final answer: *"the per-feature breakdown specifically pointing to `forward_window_component` as the source of leakage."* Trajectory: `trajectories/advanced_mixed_signal_reversion_*.json`.

## A second leakage axis: execution realism

Everything above perturbs *when* a feature is used. `execution_realism_leak.rs` (`FavorableFillReversion`) is leaky on a completely different axis: *what price* the backtest assumes it got filled at. Its feature is mechanically identical to `PriorReturnFade` -- no temporal look-ahead anywhere -- but its `Strategy::realized_return` override assumes it always buys at the bar's low and sells at the bar's high, a fill no real order type guarantees. Since `low <= close <= high` always holds, that bonus is never negative and always aligned with the position's own direction: a real, systematic Sharpe inflator baked directly into the PnL calculation, not the feature.

This is a genuine blind spot for the temporal tool, not a hypothetical one -- run it and see:

| tool | sharpe (as coded) | sharpe (counterfactual) | delta | verdict |
|---|---|---|---|---|
| `shift_test` (original vs. shifted feature) | 1.743 | -0.529 | 2.272 | **Clean** |
| `slippage_test` (declared fill vs. honest fill) | 6.156 | 1.743 | 4.412 | **Leaky** |

`shift_test`'s own numbers for this strategy are indistinguishable from `honest_mean_reversion`'s -- because `shift_test` only ever perturbs the feature array, and this strategy's feature has nothing wrong with it. `verify::slippage_test` (`src/verify/slippage.rs`) runs the same feature and offset through the backtest twice -- once with the strategy's own declared `realized_return`, once forced to the honest close-to-close default -- and reports the Sharpe delta between them, reusing the exact same "two clusters with a gap" calibration methodology as `DELTA_ROBUSTNESS_THRESHOLD` (every honest-fill strategy measures `delta == 0.0` exactly; this one measures ~4.36-4.64 across 4 independent seeds).

`advanced` now carries `slippage_test` as a third tool, and the final verdict is grounded in *either* empirical tool reporting Leaky, not just `shift_test` -- the same "two independent falsification attempts, not a primary/secondary pair" principle the original two-signal `audit` rule already established, just extended to a second axis. A real live run shows the model calling all three tools unprompted and reasoning across both axes explicitly in its final answer: *"the `shift_test` shows... the feature itself... does not suffer from temporal look-ahead bias. However, the `slippage_test` reveals a massive discrepancy... Because the strategy is leaky on the execution-realism axis, the final verdict is LEAKY."* Trajectory: `trajectories/advanced_execution_realism_leak_*.json`.

`execution_realism_leak` is deliberately excluded from the whole-vector `shift_test`/`static_scan` regression tests (both would otherwise assert a `Clean` verdict is a bug, when here it's the honest, structurally-expected result on those axes alone) -- `verify::slippage::tests` asserts the real claim instead: Clean under `shift_test`, Leaky under `slippage_test`, across 4 independent seeds. This is the concrete proof that the agentic architecture -- hypothesize, call a tool, verify empirically, reach a verdict -- generalizes to a structurally different bug class, not just more instances of temporal leakage.

## Auditing your own backtest (CSV adapter)

The limitation this project cited most often about itself: `advanced` can only audit strategies registered in `strategies::all_strategies()`, because `shift_test`'s wrapper needs a runnable `Strategy` to recompute features from. Your own backtest, in your own language, could not be checked — which is a problem, since that's who this is for.

It turns out the trait was never load-bearing. The perturbation underneath (`verify::shift_test_features`) takes a price series and an array of numbers; the `Strategy` requirement lived entirely in a convenience wrapper. So exporting two columns from *any* backtest — pandas, R, a spreadsheet — is enough:

```bash
cargo run --bin audit_csv -- examples/csv/forward_window_lookahead.csv
```
```
feature: sharpe_original=8.357 sharpe_shifted=6.718 delta=1.639 -> Leaky (implausibly high Sharpe)
VERDICT: LEAKY
```

Format: a header row, a `close` column, and at least one column named `feature*` (several are allowed — each is shift-tested independently, reusing the localization idea above). `open`/`high`/`low`/`volume` are optional. `--execution-offset` declares whether your backtest trades on the next bar (`1`, honest) or the same bar (`0`). No API key, no LLM, no network — the shift-test is deterministic, so a verdict on your data is free and reproducible.

**This is verified, not asserted.** `tests/csv_adapter_equivalence.rs` exports every seeded strategy to CSV and checks the CSV path reproduces the Rust-native `shift_test` result *bit-for-bit*, then confirms the CSV path independently classifies the battery correctly on its own.

Two honest limits, printed by the tool itself on every run rather than buried here: only the temporal axis is checked (`slippage_test` needs your fill logic, `static_scan` needs Rust source), and `IMPLAUSIBLE_SHARPE_CEILING` was calibrated on this crate's synthetic generator, *not* your market — the delta signal is relative to your own series and transfers, the absolute ceiling does not.

## A false positive we found by building the above

Testing the CSV adapter against a throwaway random series flagged an obviously-honest feature as Leaky. That was not a bug in the adapter — it is a real, previously-unnoticed failure mode in the core `audit` rule, and it survives on every seed:

| | sharpe (original) | delta | verdict | truth |
|---|---|---|---|---|
| honest strategy with **no edge** (lag-5, only past data, honest offset) | -0.358 | -0.071 | **Leaky** | Clean |

The delta signal asks *"did shifting destroy this strategy's edge?"* — which is only meaningful if there was an edge. A feature with no predictive power is **trivially** robust to the shift: nothing collapses because nothing was there. The seeded battery never exposed this because both honest controls have a genuine AR(1) edge (Sharpe ~1.74, delta ~2.27), so the failure mode sat in a blind spot of our own test design.

It is now pinned as `verify::tests::known_limitation_no_edge_honest_strategy_is_misclassified_as_leaky` — a test that asserts the *wrong* answer on purpose, so the limitation cannot be forgotten or silently regressed. `audit_csv` guards against it for external data (`NO_EDGE_SHARPE`, which downgrades such a column to inconclusive with a printed warning); `verify::audit` itself is deliberately unchanged, because v1 is tagged and changing a calibrated threshold is a v2 decision, not a quiet patch.

This is the second time this project's core verifier was caught by adversarial testing rather than by reasoning about it (the first was the one-bar blind spot, CHANGELOG Iteration 2), which is really the whole Hot Take, demonstrated twice.

## Trajectory logging

Every run of `baseline` or `advanced` writes one structured JSON trace to `trajectories/` — reasoning, tool call, tool output, verification, final verdict, in the order they actually happened. Schema and rationale: [agents.md](agents.md) §4. This is graded evidence (Deliverable 4 in [instructions.md](instructions.md)), not incidental logging: it's how a verdict is shown to have actually come from the tool-use loop rather than a plausible-sounding summary.

## Reproducing this

Full step-by-step instructions, including hardware/runtime/cost estimates: [REPRODUCTION.md](REPRODUCTION.md). Short version:

```bash
cd frontier-challenge
cargo test --lib              # deterministic engine + shift-test evidence, no network, no API key
cp .env.example .env          # fill in ANTHROPIC_API_KEY (or GEMINI_API_KEY + LLM_PROVIDER=gemini)
cargo run --bin baseline  -- src/strategies/forward_window_lookahead.rs
cargo run --bin advanced  -- src/strategies/forward_window_lookahead.rs
cargo test --test acceptance -- --nocapture   # full baseline-vs-advanced accuracy table
```

## Live evaluation results

Real runs, `LLM_PROVIDER=gemini` / `gemini-3.5-flash-lite`, sanitized source (comments and identifying names stripped, per above). Latest full run, after adding `static_scan`, against the original 5-strategy battery:

| strategy | truth | baseline | advanced |
|---|---|---|---|
| forward_window_lookahead | leaky | leaky | leaky |
| global_normalization_leak | leaky | leaky | leaky |
| execution_timing_mismatch | leaky | leaky | leaky |
| honest_mean_reversion | clean | clean | clean |
| honest_trailing_window | clean | clean | clean |

**baseline: 5/5. advanced: 5/5**, on this run. `forward_volatility_breakout` (the 6th strategy added post-hoc in REPRODUCTION.md §8) was then run live on its own, once, for real: **baseline correct, advanced correct**, `advanced` calling `static_scan` (both `SA-FORWARD-WINDOW` and `SA-GLOBAL-STAT` fired) then `shift_test` (`delta = -0.236`) and reaching LEAKY on the first pass, no retry — see REPRODUCTION.md §8 for the full trajectory pointers. `mixed_signal_reversion` (the leak-localization fixture, see above) was likewise run live on its own: **baseline correct, advanced correct**, with `advanced`'s final answer explicitly citing the per-feature breakdown to name `forward_window_component` as the leak's actual carrier, not just "leaky somewhere." `execution_realism_leak` (the second-leakage-axis strategy, see above) was also run live on its own: **baseline correct, advanced correct**, with `advanced` calling all three tools unprompted and its final answer explicitly reasoning across both axes -- `shift_test` alone would have said Clean.

But a single run's score is the wrong thing to trust here — `execution_timing_mismatch` has now been audited live 3 separate times (2 before `static_scan` existed, 1 after), and the pattern across all 3 is the real result:

| run | baseline verdict | advanced verdict |
|---|---|---|
| 1 | clean (**wrong**) | leaky (correct, needed a self-correction retry) |
| 2 | clean (**wrong**) | leaky (correct, needed a self-correction retry) |
| 3 (after `static_scan` added) | leaky (correct) | leaky (correct, first pass, no retry) |

`baseline`'s one correct call is a single unverified LLM guess — its own trajectory says so (`confidence: "unverified (single LLM call, no empirical check)"`) — it has no way to tell "I'm right" from "I got lucky." `advanced` has been correct on all 3 runs, every time backed by the same real `shift_test` numbers, and the third run shows `static_scan` measurably reducing how much correction the agent needed to get there. All trajectories are real, in `trajectories/`, produced by `cargo test --test acceptance -- --nocapture` and individual `cargo run` invocations. Full numbers: CHANGELOG Iterations 5 and 7.

### Prior art, for real

`problem.md` names [backtest-audit](https://github.com/mythofstars/backtest-audit) (a real, installable static-analysis tool) as the prior art this differentiates against. Rather than just quoting its README's stated limitations, `eval/prior_art/` translates all 5 seeded strategies to equivalent Python/pandas and actually runs the real, `pip install`-ed tool against them (positive-control-verified: it correctly fires on its own documented `shift(-1)` trigger pattern first, confirming the setup is valid):

| strategy | truth | backtest-audit |
|---|---|---|
| window_mean_reversion | leaky | **clean (wrong)** |
| zscore_reversion | leaky | **clean (wrong)** |
| mean_deviation_crossover | leaky | **clean (wrong)** |
| prior_return_fade | clean | clean |
| trailing_mean_fade | clean | clean |

**backtest-audit: 2/5** — both true negatives, zero true positives. Every miss is structural, not a near-miss: two use idioms (`rolling(center=True)`, plain-pandas global stats) that are look-ahead bugs in substance but not the literal syntax its six rules pattern-match, and the third (execution-timing mismatch) is a category of bug none of its rules address at all. Full writeup: [eval/prior_art/README.md](eval/prior_art/README.md).

**All four approaches, same 5 strategies:**

| | backtest-audit (static, external) | our own static_scan (static, offline) | baseline (1 LLM call) | advanced (agentic + static + empirical) |
|---|---|---|---|---|
| accuracy | 2/5 | 5/5 | 4-5/5, run-dependent | 5/5, every run |
| cost per audit | free | free | 1 LLM call | 2-4 LLM calls |
| reproducible / explainable verdict | yes (fixed rules) | yes (fixed rules) | no (single unverified guess) | yes (real numbers, every run) |

The static-analysis story here isn't "static loses to LLMs" — our own `static_scan`, built for this codebase instead of pandas, matches `advanced`'s accuracy at zero LLM cost. The story is that a *fixed rulebook*, however well-built, is exactly as strong as the patterns it was written for and no stronger, which is true of `backtest-audit`, true of `static_checks.rs`, and would be true of anything built the same way — it's why `advanced` treats a clean `static_scan` as a hint, never as proof.

## Main failure mode

The shift-test, as a single fixed one-bar perturbation, is mechanically blind to bugs that are themselves exactly a one-bar offset error — shifting a strategy's features by one bar happens to *repair* an execution-timing bug rather than expose it (proof and empirical numbers in CHANGELOG Iteration 2). We patched this with a second, absolute-plausibility signal, but that's a targeted fix for the three bug categories in this v1 scope, not a general solution — a bug engineered to be simultaneously persistent under a one-bar shift *and* to keep its Sharpe inside the plausible range would beat both signals.

A second, independent failure mode was found later while building the CSV adapter: the delta signal cannot distinguish a persistent leak from a strategy that never had an edge at all, so an honest but edge-less strategy is misclassified as Leaky on every seed. See "A false positive we found by building the above" — it is pinned as a deliberately-failing-by-design regression test rather than papered over.

## Hot take

Empirical verification is not automatically superior to static analysis — it just fails *differently*, and the failure is easier to miss because it looks like rigor. A tool that actually reruns the backtest carries an unearned credibility a regex-based linter never gets, but a single fixed perturbation has exactly the same shape of blind spot static analysis has: it's a fixed rule, just a numeric one instead of a syntactic one, and it's blind to whatever sits exactly outside the specific thing it checks. We only found our own tool's blind spot because we built adversarial cases *against our own verifier* and empirically checked it before trusting it — which is the actual lesson: a verification tool needs the same adversarial scrutiny as the code it's verifying, not a pass because it "runs the code."

The corollary showed up in our own evaluation, not just the tool: the first live run scored baseline and advanced *identically*, 5/5 each — a suspiciously perfect result on a task explicitly designed to be hard for the baseline. The cause was our own strategy files quietly grading themselves (doc comments starting "LEAKY. Category 1..." and struct names like `HonestMeanReversion`, see CHANGELOG Iteration 5). A too-clean result should raise the same suspicion as a too-good Sharpe ratio does in the actual audit — and for the same reason: something is leaking information that shouldn't be there.

## Roadmap: v1 (locked) and v2

**v1 is locked as of `git tag v1`** — 8 seeded strategies, three verification tools (`shift_test`, `slippage_test`, `static_scan`), uncertainty reporting, leak localization, and full trajectory logging, all deterministic and offline except the two audit binaries' LLM calls. Every claim in this README traces to a real, reproducible command (`REPRODUCTION.md`) or a real trajectory file. Nothing below this line is built yet.

**v2 (planned): validate the calibration against real market data.** `DELTA_ROBUSTNESS_THRESHOLD` and `IMPLAUSIBLE_SHARPE_CEILING` (and `SLIPPAGE_DELTA_THRESHOLD`) were calibrated purely against this project's own synthetic AR(1) generator — small mean-reversion edge, ~1% daily vol, no fat tails, no regime shifts. The open question is whether those same thresholds still separate honest from leaky once the underlying price series has real volatility clustering instead. The plan, scoped to preserve `problem.md`'s "zero external network dependency, identical output on any machine" reproducibility guarantee:

1. Vendor one frozen historical OHLCV series (a checked-in CSV, fetched once, not a live API dependency at test time) into the engine in place of `generate_series`.
2. Reuse the exact same bug-injection methodology this project already uses — a real ground-truth "leaky" label only exists for a strategy whose bug we wrote ourselves, so real data doesn't replace the seeded strategies, it replaces the *price series underneath* them.
3. Rerun the existing offline regression suite against that series and report whether the current thresholds hold, need recalibration, or don't generalize at all — any of those three outcomes is a real, useful finding, not just a pass/fail gate.

Not started. No code, no data file, no design doc exists for this yet — this section exists so the next session picks it up as a clean, scoped iteration instead of a vague aspiration.
