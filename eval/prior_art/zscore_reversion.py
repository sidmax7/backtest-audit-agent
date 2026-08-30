"""Python/pandas translation of frontier-challenge's ZScoreReversion
(src/strategies/global_normalization_leak.rs) -- LEAKY, problem.md category 3
(improper chronological split / standardizing using future state).

Mean and std are fit over the *entire* series -- classic look-ahead via
global normalization, done with plain pandas rather than sklearn, so
backtest-audit's LEAK001/LEAK002 (which require seeing a `.fit(` or
`.fit_transform(` call) do not fire on it. This is exactly the pattern
backtest-audit's own README lists as not yet recognized.
"""

import numpy as np
import pandas as pd

TRANSACTION_COST = 0.0005


def load_prices(path):
    return pd.read_csv(path, parse_dates=["date"])


def compute_features(df):
    # BUG: computed over the whole column, including future rows relative
    # to any given timestep -- a manual normalization leak, no sklearn.
    global_mean = df["close"].mean()
    global_std = df["close"].std()
    df["feature"] = -((df["close"] - global_mean) / global_std) * 0.1
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
