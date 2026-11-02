# fmt-strategy

Initial Rust implementation of the FMT M15 session signal engine.

## Contract

- Input: completed Bybit linear perpetual BTCUSDT M15 OHLCV candles.
- Candle timestamp: UTC candle-open time, aligned to 15-minute boundaries.
- Session timezone: `America/New_York` (DST-aware).
- Pre-session range: 18:00–20:00 New York time.
- Tokyo session: 20:00–06:00 New York time, associated with the date on which 20:00 occurs.
- VWAP: cumulative typical-price (`hlc3`) × volume / volume, starting with pre-session candles and continuing through the Tokyo session.
- Signals are close-confirmed. This crate emits events; it does not execute trades.

## Important implementation notes

1. Candle data must arrive exactly once and in strictly increasing timestamp order. Duplicate or out-of-order bars are rejected.
2. Missing pre-session data does not reuse the previous session's range or VWAP.
3. The previous candle close is retained across the 19:45-to-20:00 boundary, matching Pine's `close[1]` reference on the first Tokyo candle. The logical position is reset at the next Tokyo start, not at 06:00.\n4. Exit checks run before entry checks. Therefore, if an exit and a fresh entry both qualify on one candle, the engine may emit both events in that order, matching the sequential rule evaluation.
5. Session state in this signal engine is not an exchange position ledger. The execution service must reconcile actual GMX on-chain positions and must never clear an open position merely because a session changed.
7. This crate is not yet parity-certified against the user's original Pine Script. Pine's exact bar/session semantics and edge cases must be tested against exported TradingView candles before live use.
6. This crate does not contain Bybit ingestion, GMX SDK integration, risk sizing, multi-user isolation, wallet authorization, or order execution.

## Build

```sh
cargo test -p fmt-strategy
```
