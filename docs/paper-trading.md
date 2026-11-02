# FMT paper trading ledger

## Scope

The paper ledger is a per-user simulation of the FMT signal stream. It records one open simulated position per strategy instance and closes it when the matching FMT exit event arrives.

- Entry and exit reference prices come from confirmed Bybit BTCUSDT linear perpetual M15 candles.
- Long and short gross PnL is stored as integer basis points.
- A user who enables or re-enables a strategy after the current New York session starts joins from the next session. This avoids an exit being delivered without the corresponding entry.
- REST backfill reconstructs the paper ledger after worker startup or reconnect. Replayed signal events include `origin=REST_BACKFILL` and `is_live_signal=false`.
- Live WebSocket signal events include `origin=LIVE_WEBSOCKET` and `is_live_signal=true`. **Every strategy event remains `execution_eligible=false`**; a fresh signal is not an order authorization. No execution consumer is implemented.

## PnL formula

For entry price (P_e) and exit price (P_x):

- BUY: ((P_x - P_e) / P_e 	imes 10,000) basis points.
- SELL: ((P_e - P_x) / P_e 	imes 10,000) basis points.

The value is rounded to the nearest integer basis point.

## Limitations

The ledger is not a backtest or broker statement. It does not simulate:
- GMX order placement, partial fills, or execution price.
- Trading fees, price impact, slippage, funding, borrowing costs, or liquidation.
- Collateral requirements, margin, leverage, or actual account equity.
- A position being force-closed at the end of a Tokyo session.

An open paper position remains open until a matching exit event arrives. The unique open-position constraint prevents a second simulated position from being created for the same strategy instance while one remains open.

## Data and privacy

The worker writes paper positions with the Supabase service-role key. Authenticated users receive read-only access to rows where `user_id = auth.uid()`. The table is included in Supabase Realtime for the dashboard. No browser client can create or modify paper positions.
