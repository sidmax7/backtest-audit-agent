"""Python/pandas translation of frontier-challenge's MeanDeviationCrossover
(src/strategies/execution_timing_mismatch.rs) -- LEAKY, problem.md category 2
(execution timing mismatch: trades at the current bar's close instead of the
next bar's open).

The feature itself is an honest, causal trailing-window read. The bug is
that the position is applied to the *same* bar's return instead of the next
bar's -- no `.shift(-n)`, no `.pct_change(-n)`, no `.fit(` anywhere in this
file. None of backtest-audit's six rules target execution/PnL timing at all,
so this category is structurally outside what it checks for, not just a
pattern it happens to miss.
"""

import numpy as np
import pandas as pd

TRANSACTION_COST = 0.0005


def load_prices(path):
    return pd.read_csv(path, parse_dates=["date"])


def compute_features(df):
    trailing_mean = df["close"].rolling(window=5).mean()
    df["feature"] = (df["close"] - trailing_mean) / trailing_mean
    return df


def backtest(df):
    df["position"] = df["feature"].clip(-1, 1)
    df["ret"] = df["close"].pct_change()
    # BUG: no .shift(1) here -- position for bar i is applied to bar i's own
    # already-realized return instead of the next bar's.
    df["pnl"] = df["position"] * df["ret"] - TRANSACTION_COST
    return df


def sharpe_ratio(pnl):
    return (pnl.mean() / pnl.std()) * np.sqrt(252)


if __name__ == "__main__":
    df = load_prices("prices.csv")
    df = compute_features(df)
    df = backtest(df)
    print("Sharpe:", sharpe_ratio(df["pnl"]))
