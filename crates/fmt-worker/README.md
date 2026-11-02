# fmt-worker

First operational worker prototype for the confirmed Bybit M15 data stream.

## Current behavior

1. Verifies that Bybit BTCUSDT is an active `LinearPerpetual` instrument with status `Trading`.
2. Loads up to 48 hours of closed M15 candles through REST to reconstruct the current FMT session state.
3. Connects to the public linear WebSocket topic `kline.15.BTCUSDT`.
4. Passes only `confirm=true` candles to the Rust FMT engine.
5. Upserts confirmed OHLCV candles into the shared `market_candles` table.
6. Uses REST backfill only to rebuild strategy state and persist candle history; historical signals are suppressed.
7. Fans live WebSocket FMT signals out to enabled user instances that were active before the current session began; mid-session activations join at the next session to avoid orphan exits.
8. Persists user-scoped events.
9. Reconnects only after attempting REST backfill; the engine rejects missing or out-of-order candles.
10. It does not create orders.

## Environment

- `BYBIT_ENV=production` or `BYBIT_ENV=testnet`
- `SUPABASE_URL` and `SUPABASE_SERVICE_ROLE_KEY` (required for candle/event persistence and dashboard Realtime)
- `RUST_LOG=fmt_worker=info` (optional)

## Important limitations

- Supabase writes use the service-role REST API and retry with exponential backoff while the worker process remains alive. A bounded in-memory queue applies backpressure. There is no durable queue or dead-letter queue; after a restart, recent candles are replayed to rebuild state, but historical signals are intentionally suppressed.
- Multi-user event fan-out is implemented; per-user worker isolation and independent strategy parameter sets are not.
- Reconnect/backfill is a first pass; production deployment still needs supervised process restarts, alerting, durable checkpoints, rate-limit handling, and load tests.
- There is no GMX execution path in this worker.
