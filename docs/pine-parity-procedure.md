# Pine Script parity validation procedure

Daniel supplied the complete Pine Script v5 source. The exact source is archived at `docs/reference/fmt-tokyo-original.pine`. Exact strategy equivalence still requires a TradingView M15 OHLCV export to compare numeric VWAP values, levels, signals, and transitions.

## Required user inputs

1. A TradingView chart-data CSV exported from `BYBIT:BTCUSDT`, timeframe M15, with timestamp, open, high, low, close, and volume.
2. A date range containing several complete Tokyo sessions, at least one session with a buy signal, one with a sell signal, and at least one exit. If possible, include a New York DST transition period.
3. A screenshot or CSV of the Pine indicator's signal timestamps and side, if the indicator exposes them.

## Comparison

The parity harness must compare, candle by candle:

- New York local session classification and session anchor date.
- Eight pre-session bars, pre-session high, and pre-session low.
- Cumulative `hlc3 * volume`, cumulative volume, and anchored VWAP.
- Previous-close lookup and confirmed-candle timing.
- Buy/sell conditions and exit conditions.
- Signal ordering when an exit and a new entry are both true on one candle.
- Overnight continuity and daylight-saving transitions.

## Acceptance rule

The Rust engine is accepted as strategy-equivalent only when every tested level, VWAP value (within an explicitly documented floating-point tolerance), signal timestamp, side, and state transition matches the Pine reference. Any mismatch must be explained and approved by the user before changing either implementation.

Until this procedure passes, FMT signal generation is a development translation and real execution stays disabled.
