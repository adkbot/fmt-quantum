# fmt-api

Initial Rust HTTP API service.

## Routes

- `GET /health`: public health/status response. It always reports `execution_enabled: false`.
- `GET /v1/me`: protected identity route. Requires a Supabase access token in the Bearer authorization header.
- `GET /v1/risk-config`: reads the authenticated user's risk limits.
- `PUT /v1/risk-config`: validates and upserts the authenticated user's risk limits; the maximum open-position count is hard-capped at 8.

The API validates the access token against Supabase Auth's `/auth/v1/user` endpoint and scopes each risk-config query/upsert to that authenticated user ID. Monetary fields use integer USD cents; leverage and collateral ratios use basis points.

## Required environment

- `SUPABASE_URL`
- `SUPABASE_ANON_KEY`
- `SUPABASE_SERVICE_ROLE_KEY` (server only; required for risk-config persistence)
- `API_BIND` (defaults to `127.0.0.1:8080`)
- `CORS_ORIGIN` (defaults to `http://localhost:3000`)

The service-role key must never be sent to the browser or committed to Git.

## Not yet implemented

No wallet signature challenge, GMX position reconciliation, execution endpoint, or trading routes are included. Risk configuration reads/writes use the Supabase REST API with the service-role key on the server. This API cannot place orders.
