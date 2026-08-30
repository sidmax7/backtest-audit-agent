"""Python/pandas translation of frontier-challenge's PriorReturnFade
(src/strategies/honest_mean_reversion.rs) -- CLEAN control.

Bets against the previous bar's already-realized return. Properly lagged:
the position for bar i is built from bar i's feature but only applied via
`.shift(1)` to bar i+1's return -- the honest "decide before, earn after"
contract, same as the Rust engine's default `execution_offset() == 1`.
"""

import numpy as np
import pandas as pd

TRANSACTION_COST = 0.0005


def load_prices(path):
    return pd.read_csv(path, parse_dates=["date"])


def compute_features(df):
    prev_ret = df["close"].pct_change()
    df["feature"] = -prev_ret * 5.0
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
