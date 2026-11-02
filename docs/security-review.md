# Security review status

This is an implementation checklist, not an independent audit or production approval.

## Implemented in the repository

- Supabase Auth access tokens are validated by the Rust API against `/auth/v1/user`.
- User-scoped reads and writes use the authenticated Supabase user ID, not a user ID supplied by the browser.
- Supabase service-role credentials are used only by server-side Rust services and the worker.
- The frontend receives only the Supabase anon/public key.
- Wallet ownership uses a random 32-byte nonce, a five-minute expiry, an EIP-191 personal-sign message, server-side signer recovery, and atomic one-time challenge consumption.
- Active wallet challenges are limited to five per user/address.
- Wallet signature verification does not request token approvals or submit transactions.
- Wallet challenge rows have RLS enabled with no authenticated-client policies.
- Risk and strategy configuration writes go through the authenticated Rust API; direct authenticated-client write policies are absent.
- Monetary risk limits use integer cents and leverage/collateral ratios use basis points.
- The risk engine hard-caps concurrent positions at eight and returns a deferral rather than closing an existing position.
- GMX adapter HTTP routes are read-only, loopback-bound by default, and protected by a server-side internal API key.
- The API proxies GMX account reads only for wallet addresses verified and associated with the authenticated user.
- No private keys, seed phrases, or delegated signing keys are stored.

## Not yet production-ready

- No independent security audit or penetration test has been completed.
- No live GMX order execution, delegated subaccount authorization, or transaction signing service exists.
- No rate limiting exists globally on the API; only active wallet-challenge creation has a per-address cap.
- No durable event queue, dead-letter queue, or guaranteed replay exists for worker-to-Supabase event writes.
- No distributed lock or worker leader election exists.
- No server-side idempotency permit has been implemented for future execution.
- Risk evaluation is not yet connected to authoritative live GMX equity, collateral, positions, funding, fees, or liquidation distance.
- The frontend has no CSRF/session-cookie architecture because it currently uses the browser Supabase client; review before introducing cookie-based auth.
- Wallet unlink UI, expired challenge cleanup, alerting, secret rotation, and production key management remain to be implemented.
- No live trading is enabled. Keep all execution flags disabled until the missing controls, tests, and audits are complete.
