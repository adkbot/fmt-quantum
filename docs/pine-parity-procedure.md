# Pine Script parity validation procedure

Exact strategy equivalence cannot be claimed from a prose summary alone. The literal Pine Script v5 source is not currently present in the repository or the accessible Library.

## Required user inputs

1. The complete Pine Script v5 source, including all inputs, session strings, timezone, state variables, plot/alert conditions, and statement order.
2. A TradingView chart-data CSV exported from `BYBIT:BTCUSDT`, timeframe M15, with timestamp, open, high, low, close, and volume.
3. A date range containing several complete Tokyo sessions, at least one session with a buy signal, one with a sell signal, and at least one exit. If possible, include a New York DST transition period.
4. A screenshot or CSV of the Pine indicator's signal timestamps and side, if the indicator exposes them.

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
