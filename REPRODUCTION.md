# Reproduction Guide

Step-by-step instructions to build and run this from a blank machine. See [README.md](README.md) for what this is and why, and [agents.md](agents.md) for the build order this was developed in.

## Requirements

- **Rust toolchain:** built and tested against `rustc 1.98.0` / `cargo 1.98.0`. Any reasonably recent stable toolchain (1.75+) should work — the code has no nightly features.
- **Hardware:** trivial. The whole deterministic test suite (engine + strategies + shift-test) runs in well under a second on a single core; no GPU, no special memory requirements.
- **Network:** required only for `cargo fetch`/`cargo build` (crates.io) and for the two binaries' LLM calls. The deterministic core (`cargo test --lib`) makes zero network calls.
- **API key (optional, only for the LLM-backed parts):** an Anthropic API key (default) or a Gemini API key. Nothing else — no market data provider, no other external service.

## 1. Clone and build

```bash
cd frontier-challenge
cargo build
```

First build fetches and compiles dependencies (reqwest, serde, clap, etc.) — expect 1-2 minutes depending on connection speed. Subsequent builds are incremental and fast (a few hundred ms).

## 2. Run the deterministic evidence (no API key needed)

This is the core empirical claim of the whole project — that the shift-test correctly separates leaky from clean strategies — and it's fully offline and deterministic:

```bash
cargo test --lib
```

Expect `34 passed; 0 failed` in well under a second. The key tests are `verify::tests::shift_test_correctly_classifies_every_seeded_strategy_across_seeds`, which checks 7 of the 8 seeded strategies against 4 independent seeds (28/28 correct), and `verify::slippage::tests::slippage_test_catches_the_favorable_fill_leak_shift_test_is_blind_to`, which covers the 8th (`execution_realism_leak`) on the axis it actually leaks on — it is deliberately, correctly Clean under the temporal shift-test alone, so it is excluded from that test rather than asserted against it. See README "A second leakage axis" for why.

To see the raw numbers behind that table (per-strategy Sharpe before/after the shift, across seeds):

```bash
cargo run --example inspect
```

## 3. Configure an LLM provider (needed for the rest)

```bash
cp .env.example .env
```

Edit `.env`:
- **Anthropic (default):** set `ANTHROPIC_API_KEY`. Optionally set `ANTHROPIC_MODEL` (defaults to `claude-opus-5`).
- **Gemini:** set `LLM_PROVIDER=gemini` and `GEMINI_API_KEY`. Optionally set `GEMINI_MODEL` (defaults to `gemini-3.5-flash-lite`; Google ships new model generations quickly, so check what's current for your key if the default 404s -- `gemini-2.5-flash`'s free tier caps at only 20 requests/*day*, too low to run the full evaluation in one sitting, which is why the default isn't that one).

`.env` is gitignored — never commit real keys.

## 4. Single command: run the baseline

```bash
cargo run --bin baseline -- src/strategies/forward_window_lookahead.rs
```

Prints a verdict from a single LLM call on the raw source, with no empirical check behind it, and writes a trajectory to `trajectories/`. Swap the path for any file under `src/strategies/` to audit a different seeded strategy.

## 5. Single command: run the advanced agent

```bash
cargo run --bin advanced -- src/strategies/forward_window_lookahead.rs
```

Prints a verdict backed by an actual `shift_test` rerun, including the real Sharpe delta, and writes its own (richer) trajectory to `trajectories/`.

## 6. Full evaluation: baseline vs. advanced accuracy

```bash
cargo test --test acceptance -- --nocapture
```

Runs both binaries against all 8 seeded strategies and prints a side-by-side accuracy table. This makes real LLM calls (roughly 25-35 requests total: 8 for baseline, 2-4 each for advanced depending on tool use and whether the self-correction retry fires) and is skipped — not failed — if no API key is configured, so a plain `cargo test` (without `--test acceptance` explicitly) stays free. An `INCONCLUSIVE` verdict from `advanced` is reported in the table and excluded from the accuracy denominator rather than scored as wrong — see README "Uncertainty reporting".

## 7. Optional: reproduce the prior-art (`backtest-audit`) comparison

This is Python tooling, kept separate from the Rust crate on purpose (`problem.md`'s reproducibility section scopes the self-contained-engine requirement to the Rust side; this comparison is evidence about a *different* tool, not part of this project's own reproduction path). Requires Python 3.8+ and `pip`.

```bash
python3 -m venv venv         # create the venv BEFORE cd'ing in -- see caution below
source venv/bin/activate
pip install backtest-audit
cd ../eval/prior_art
backtest-audit check .
```

Expect `✓ No issues found.` — i.e. 0 of the 3 leaky strategies flagged. For an explicit per-file breakdown instead of one aggregate line, run `./check.sh` from `eval/prior_art/` — a thin wrapper around `backtest-audit check . --format json` (requires `jq`) that prints one confirmation line per filename instead of a bare `[]`. Full writeup and a positive-control sanity check (confirming the tool is actually working, not just silent) in `eval/prior_art/README.md`.

> [!CAUTION]
> Order matters here. `backtest-audit check .` scans everything under the current directory, including a `venv/` folder if one happens to be inside it -- and pip's and rich's own vendored source trips `backtest-audit`'s own rules (`STAT002`, `LEAK001`), producing unrelated false positives that have nothing to do with the seeded strategies. Create the venv *before* `cd`-ing into `eval/prior_art`, exactly as above, so it never ends up inside the directory being scanned. (Found by literally re-running these exact commands in this order and getting "5 errors, 7 warnings" instead of "No issues found" -- fixed here rather than left for the next person to hit.)

## 8. Extending this: adding a 6th strategy

Every test, example, and binary in this repo iterates `strategies::all_strategies()` — nothing else enumerates strategies by name. So adding one is three steps, no dynamic loading or plugin system required:

1. **Implement `Strategy`** in a new file under `src/strategies/`. Minimum surface: `name()` and `compute_features()`; `execution_offset()` defaults to the honest `1` unless you override it.
2. **Register it** as one more `StrategyEntry` in `all_strategies()` (`src/strategies/mod.rs`) — struct instance, `is_leaky` ground truth, `source_path`.
3. **Run it**: `cargo test --lib` immediately re-checks it against the shift-test regression suite (all 4 seeds) and the static scanner; `cargo run --example inspect` prints its real Sharpe numbers; `cargo run --bin advanced -- src/strategies/<file>.rs` audits it live.

No other file needs to change — not the acceptance test, not `static_checks.rs`, not the trajectory schema.

As proof rather than just a claim, `forward_volatility_breakout.rs` was added this way: same off-by-one window bug as `forward_window_lookahead.rs` (the window reaches forward past bar `i`), but computed as a volatility-breakout signal instead of mean-reversion, so it's a genuinely different strategy, not a copy. Zero other files changed except the two steps above. Result, seed 42:

```
strategy                       sharpe_0   sharpe_1      delta   leaky?  verdict
forward_volatility_breakout       0.318      0.554     -0.236     true Leaky
```

Correctly flagged Leaky (`delta = -0.236`, comfortably under the `1.0` robustness threshold — the shift barely dents it, same signature as the other forward-window leak). The offline suite went from 5 to 6 strategies and stayed green: `cargo test --lib` → `21 passed; 0 failed`, including `shift_test_correctly_classifies_every_seeded_strategy_across_seeds` (now 6 strategies × 4 seeds = 24/24) and `static_checks::scan_correctly_classifies_every_seeded_strategy` (now 6/6).

Then run live, once, for real: `cargo run --bin baseline -- src/strategies/forward_volatility_breakout.rs` and `cargo run --bin advanced -- src/strategies/forward_volatility_breakout.rs` (`gemini-3.5-flash-lite`). Both correctly returned `VERDICT: LEAKY`. `advanced` called `static_scan` first (`SA-FORWARD-WINDOW` and `SA-GLOBAL-STAT` both fired), then `shift_test` (`sharpe_original=0.318 sharpe_shifted=0.554 delta=-0.236`), and reached the correct verdict on the first pass — no self-correction retry needed. Trajectories: `trajectories/baseline_forward_volatility_breakout_*.json`, `trajectories/advanced_forward_volatility_breakout_*.json`.

## 9. Audit your own backtest from CSV (no API key, no LLM)

The other steps audit this crate's own seeded strategies. This one audits *yours*, from any language:

```bash
cargo run --bin audit_csv -- examples/csv/forward_window_lookahead.csv   # -> LEAKY
cargo run --bin audit_csv -- examples/csv/honest_mean_reversion.csv      # -> CLEAN
```

Export a CSV with a header row, a `close` column, and at least one column named `feature*` (several allowed — each is shift-tested separately). `open`/`high`/`low`/`volume` optional. Add `--execution-offset 0` if your backtest trades on the same bar its feature was computed from (default `1`, the honest next-bar convention). From pandas that is one line: `df[["close", "feature"]].to_csv("export.csv", index=False)`.

Deterministic and free — no API key, no network. Regenerate the bundled examples with `cargo run --example export_csv`.

Correctness is verified rather than claimed: `cargo test --test csv_adapter_equivalence` exports every seeded strategy to CSV and asserts the CSV path reproduces the Rust-native `shift_test` numbers bit-for-bit, then that it classifies the battery correctly on its own.

The tool prints its own limits on every run: only the temporal axis applies to a CSV export, and the absolute-Sharpe ceiling was calibrated on this crate's synthetic market, not yours. It also warns when a feature has essentially no edge, where the shift-test is structurally uninformative — see README "A false positive we found by building the above".

## 10. Three further extensions: uncertainty, localization, a second leakage axis

No new reproduction commands — the same `cargo test --lib`, `cargo run --example inspect`, and `cargo run --bin baseline/advanced -- <file>` from the steps above cover all three, since everything routes through `strategies::all_strategies()` and the existing binaries. Full writeups (hypothesis, implementation, real measured evidence) are CHANGELOG Iterations 9, 10, and 11; summarized in README under "Uncertainty reporting," "Leak localization," and "A second leakage axis: execution realism." Briefly:

- **Uncertainty reporting** (`verify::combine_signals`, `AgentVerdict::Inconclusive`): `advanced` no longer forces LEAKY/CLEAN when `static_scan` flags something but `shift_test` says Clean anyway — proven with a dedicated offline fixture (real source, real `syn` parse, real `shift_test`), not a mock; zero effect on any existing strategy's verdict.
- **Leak localization** (`Strategy::feature_components`, `verify::localize`): `shift_test` can now run per named feature-component. `mixed_signal_reversion.rs` (registered, `is_leaky: true`, 7th strategy) is the concrete discriminating proof — genuinely Leaky as a whole, but `localize` correctly separates its honest and forward-window components and verdicts them differently.
- **A second leakage axis** (`Strategy::realized_return`, `verify::slippage_test`): a structurally different bug class — an unrealistic fill-price assumption baked into the PnL calculation, not the feature. `execution_realism_leak.rs` (registered, `is_leaky: true`, 8th strategy) is genuinely Clean under `shift_test` alone (identical numbers to `honest_mean_reversion`) and only correctly Leaky once `slippage_test`, a new third tool in `advanced`, is in the mix — deliberately excluded from the two whole-vector-only regression tests, with a dedicated test in `verify::slippage::tests` asserting the real, combined claim instead.

All three were run live (baseline + advanced) against their respective new/affected strategy, same workflow as `forward_volatility_breakout` above — see CHANGELOG for the exact trajectory filenames and quoted model output.

## Runtime and cost estimate (measured, not projected)

Real measured token counts, not projections. The 5-strategy figures are summed from the 10 trajectory files written by an actual `cargo test --test acceptance -- --nocapture` run (`gemini-3.5-flash-lite`, with the `static_scan` tool); the 3 later strategies were each run live individually, and their most recent trajectory files are summed here:

| | input tokens | output tokens |
|---|---|---|
| baseline, original 5 (one full harness run) | 1,542 | 354 |
| advanced, original 5 (one full harness run) | 14,776 | 960 |
| baseline, 3 later strategies (individual runs) | 1,452 | 239 |
| advanced, 3 later strategies (individual runs) | 13,264 | 606 |
| **total across all 8** | **31,034** | **2,159** |

- **Deterministic suite** (`cargo test --lib`, `cargo run --example inspect`): under 1 second, $0.
- **A full 8-strategy evaluation run:** ~31K input + ~2.2K output tokens across 16 binary invocations. At Claude Opus 5 pricing ($5/$25 per MTok in/out) that's roughly $0.21; Gemini's flash-lite tier is a small fraction of a cent. Either way, a full run costs well under $1. Note the per-strategy `advanced` cost rose as tools were added (`execution_realism_leak` is the priciest at ~6.1K input, since the model calls all three tools) — budget on the newer figures, not the original five.
- Wall-clock is dominated by LLM latency, not compute -- individual runs took 1-3.5s each, except one that took ~105s after hitting the free-tier per-minute rate limit and waiting out the server-specified retry delay (see CHANGELOG Iteration 5). Budget a couple of minutes for a full run to be safe against rate limits on a free-tier key.
