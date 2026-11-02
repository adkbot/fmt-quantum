# Bybit market-data feed

The initial FMT reference stream is **BTCUSDT linear perpetual, M15**.

The Rust crate `bybit-feed` uses the public Bybit V5 REST API:

- `GET /v5/market/instruments-info?category=linear&symbol=BTCUSDT` to verify that the instrument is `LinearPerpetual` and `Trading`.
- `GET /v5/market/kline?category=linear&symbol=BTCUSDT&interval=15` for OHLCV.
- Bybit returns klines newest-first; the adapter sorts them oldest-first and validates the candle fields before returning them to the strategy engine.
- For USDT linear contracts, Bybit documents kline volume in base-coin units (BTC for BTCUSDT).

The feed uses public market-data endpoints and does not require or store a Bybit API key. The adapter is not an execution client.

## Operational requirements still outstanding

- Add the Bybit public WebSocket kline stream for low-latency updates.
- Gate signals on confirmed/closed M15 candles; never process an in-progress candle as final.
- Add reconnect, heartbeat, backoff, gap detection, and REST backfill after reconnect.
- Persist source timestamps and detect duplicate/missing candles.
- Compare Bybit reference prices with GMX oracle/market prices and enforce a configured divergence threshold before any execution.
- Run parity tests against TradingView-exported OHLCV and the original Pine indicator.

Official API references: [Get Kline](https://bybit-exchange.github.io/docs/v5/market/kline) and [Get Instruments Info](https://bybit-exchange.github.io/docs/v5/market/instrument).
