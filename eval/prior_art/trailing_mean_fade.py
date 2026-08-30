"""Python/pandas translation of frontier-challenge's TrailingMeanFade
(src/strategies/honest_trailing_window.rs) -- CLEAN control.

The honest counterpart of mean_deviation_crossover.py: identical trailing-
window feature, but the position is correctly lagged with `.shift(1)`
before being applied to the return, instead of applied to the same bar.
"""

import numpy as np
import pandas as pd

TRANSACTION_COST = 0.0005


def load_prices(path):
    return pd.read_csv(path, parse_dates=["date"])


def compute_features(df):
    trailing_mean = df["close"].rolling(window=2).mean()
    df["feature"] = (trailing_mean - df["close"]) / trailing_mean
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
