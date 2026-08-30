"""Python/pandas translation of frontier-challenge's WindowMeanReversion
(src/strategies/forward_window_lookahead.rs) -- LEAKY, problem.md category 1.

The rolling window is centered, so it includes bars *after* the current one.
This is a very common real-world way this bug gets introduced -- and it does
not use pandas' `.shift(-n)`, so backtest-audit's LAB001 (which pattern-
matches `shift(-n)` specifically) does not fire on it.
"""

import numpy as np
import pandas as pd

TRANSACTION_COST = 0.0005


def load_prices(path):
    return pd.read_csv(path, parse_dates=["date"])


def compute_features(df):
    # BUG: center=True means this window includes future bars.
    window_mean = df["close"].rolling(window=5, center=True).mean()
    df["feature"] = (window_mean - df["close"]) / df["close"]
    return df


def backtest(df):
    df["position"] = df["feature"].shift(1).clip(-1, 1)
    df["ret"] = df["close"].pct_change()
    df["pnl"] = df["position"] * df["ret"] - TRANSACTION_COST
    return df


def sharpe_ratio(pnl):
    return (pnl.mean() / pnl.std()) * np.sqrt(252)


if __name__ == "__main__":
    df = load_prices("prices.csv")
    df = compute_features(df)
    df = backtest(df)
    print("Sharpe:", sharpe_ratio(df["pnl"]))
