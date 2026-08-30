# Prior-Art Comparison: `backtest-audit` Against This Project's Test Battery

`problem.md` names [backtest-audit](https://github.com/mythofstars/backtest-audit) as the prior art this project differentiates against, and quotes its own documented limitations. This directory makes that claim concrete and reproducible instead of asserted: real Python/pandas translations of all 5 seeded strategies from `src/strategies/`, actually checked with the real, installed tool.

## What's here

Each file is a faithful pandas translation of the matching Rust strategy (see each file's docstring for the exact correspondence and why it does or doesn't trip a specific rule):

| file | Rust source | ground truth |
|---|---|---|
| `window_mean_reversion.py` | `forward_window_lookahead.rs` | leaky |
| `zscore_reversion.py` | `global_normalization_leak.rs` | leaky |
| `mean_deviation_crossover.py` | `execution_timing_mismatch.rs` | leaky |
| `prior_return_fade.py` | `honest_mean_reversion.rs` | clean |
| `trailing_mean_fade.py` | `honest_trailing_window.rs` | clean |

These are static text files for an AST tool to parse -- `backtest-audit` never executes them (that's its whole design), so there's no `prices.csv` to actually run them against.

## Reproducing

```bash
python3 -m venv venv
source venv/bin/activate
pip install backtest-audit
cd eval/prior_art
backtest-audit check .
```

## Result (real, captured 2026-08-29)

```
$ backtest-audit check .
✓  No issues found.
```

Confirmed per-file with `--format json` too -- every file returns `[]` (zero issues), including all three leaky ones. `./check.sh` (requires `jq`) reformats that same JSON into one explicit confirmation line per filename, e.g. `window_mean_reversion.py: ✓ checked, no issues found`, so the per-file result doesn't need a bare `[]` explained out loud.

**Positive control**, to confirm this isn't a broken install or invocation -- `backtest-audit`'s own documented trigger pattern fires correctly:

```python
df["signal"] = df["close"].shift(-1)
```
```
[LAB001] Negative shift — future data pulled into current timestep  line 3
1 error
```

## Scorecard

| strategy | truth | backtest-audit | why it misses |
|---|---|---|---|
| window_mean_reversion | leaky | **clean (wrong)** | `.rolling(center=True)` isn't `.shift(-n)` -- LAB001 only pattern-matches the literal negative-shift idiom |
| zscore_reversion | leaky | **clean (wrong)** | plain pandas `.mean()`/`.std()` over the whole column, no `.fit(`/`.fit_transform(` call -- LEAK001/002 require seeing an sklearn-style call |
| mean_deviation_crossover | leaky | **clean (wrong)** | execution/PnL timing isn't covered by any of the 6 rules at all -- structurally outside what static analysis here checks for |
| prior_return_fade | clean | clean (correct) | -- |
| trailing_mean_fade | clean | clean (correct) | -- |

**backtest-audit: 2/5** -- both true negatives, zero true positives. Compare against this project's own live results (README.md's "Live evaluation results"): **baseline (single LLM call): 4/5, advanced (agentic + shift-test): 5/5.**

This isn't a cherry-picked failure. Every miss traces to a documented, structural limitation, not an edge case: `window_mean_reversion` and `zscore_reversion` are look-ahead/leakage bugs written in idioms that simply aren't the specific ones backtest-audit's six fixed rules pattern-match against, and `mean_deviation_crossover` is a whole *category* of bug (execution/PnL timing) none of its rules address at all. `problem.md`'s framing -- "does not execute code, cannot detect patterns beyond its static rulebook" -- is exactly what this run shows happening, with a real accuracy number instead of a description.
