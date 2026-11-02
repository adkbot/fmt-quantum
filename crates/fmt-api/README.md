# fmt-api

Initial Rust HTTP API service.

## Routes

- `GET /health`: public health/status response. It always reports `execution_enabled: false`.
- `GET /v1/me`: protected identity route. Requires a Supabase access token in the Bearer authorization header.
- `GET /v1/risk-config`: reads the authenticated user's risk limits.
- `GET /v1/strategy`: reads the authenticated user's FMT Tóquio BTCUSDT M15 monitoring instance.
- `PUT /v1/strategy`: enables/disables that user's monitoring instance. Enabling requires a saved risk configuration.
- `PUT /v1/risk-config`: validates and upserts the authenticated user's risk limits; the maximum open-position count is hard-capped at 8.
- `GET /v1/wallets`: lists the authenticated user's verified wallet addresses.
- `GET /v1/gmx/markets`: proxies the GMX V2 market catalog to an authenticated user.
- `GET /v1/gmx/account?wallet_address=...`: proxies account positions/orders/balances only when the address is verified and linked to that user.
- `POST /v1/wallet/challenge`: creates a random, five-minute, one-time EIP-191 wallet ownership challenge for Arbitrum One.
- `POST /v1/wallet/verify`: recovers the signer address, atomically consumes the challenge, and upserts the verified wallet association.

The API validates the Supabase access token against Supabase Auth's `/auth/v1/user` endpoint and scopes user records to that authenticated ID. Wallet signatures prove address ownership only; they do not authorize trades, approvals, deposits, withdrawals, or delegated access.

Monetary risk limits use integer USD cents; leverage and collateral ratios use basis points.

## Required environment

- `SUPABASE_URL`
- `SUPABASE_ANON_KEY`
- `SUPABASE_SERVICE_ROLE_KEY` (server only)
- `GMX_ADAPTER_URL` (defaults to `http://127.0.0.1:8090`)
- `GMX_ADAPTER_INTERNAL_KEY` (same secret configured in the private TypeScript adapter)
- `API_BIND` (defaults to `127.0.0.1:8080`)
- `CORS_ORIGIN` (defaults to `http://localhost:3000`)

The service-role key must never be sent to the browser or committed to Git.

## Not yet implemented

GMX read-only market and account routes are included through the private TypeScript adapter. No GMX position reconciliation, execution endpoint, delegated subaccount authorization, or trading routes are included. Risk configuration and wallet associations are persisted through the Supabase REST API with the service-role key on the server. This API cannot place orders.
