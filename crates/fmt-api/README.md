# fmt-api

First Rust HTTP service scaffold.

## Routes

- `GET /health`: public health/status response. It always reports `execution_enabled: false`.
- `GET /v1/me`: protected identity route. Requires a Supabase access token in the Bearer authorization header.

The `/v1/me` handler validates the token against Supabase Auth's `/auth/v1/user` endpoint and returns the authenticated user ID.

## Required environment

- `SUPABASE_URL`
- `SUPABASE_ANON_KEY`
- `API_BIND` (defaults to `127.0.0.1:8080`)
- `CORS_ORIGIN` (defaults to `http://localhost:3000`)

The service-role key is not used by this service and must never be sent to the browser.

## Not yet implemented

No database access, user risk-config routes, wallet signature challenge, Bybit feed worker, GMX position reconciliation, execution endpoint, or trading routes are included. This API cannot place orders.
