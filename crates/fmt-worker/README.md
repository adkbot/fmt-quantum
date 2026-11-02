# fmt-worker

First operational worker prototype for the confirmed Bybit M15 data stream.

## Current behavior

1. Verifies that Bybit BTCUSDT is an active `LinearPerpetual` instrument with status `Trading`.
2. Loads up to 48 hours of closed M15 candles through REST to reconstruct the current FMT session state.
3. Connects to the public linear WebSocket topic `kline.15.BTCUSDT`.
4. Passes only `confirm=true` candles to the Rust FMT engine.
5. Logs strategy events; it does not create orders.
6. On disconnect, reconnects only after attempting REST backfill. The engine rejects missing or out-of-order candles.

## Environment

- `BYBIT_ENV=production` or `BYBIT_ENV=testnet`
- `RUST_LOG=fmt_worker=info` (optional)

## Important limitations

- Events are logged only; they are not persisted to Supabase yet.
- This worker is single-process and single-stream. Multi-user fan-out and per-user worker isolation are not implemented.
- Reconnect/backfill is a first pass; production deployment still needs supervised process restarts, alerting, durable checkpoints, rate-limit handling, and load tests.
- There is no GMX execution path in this worker.
