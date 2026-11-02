# Risk engine

The `fmt-risk` crate is the first fail-closed risk layer. It returns an approval, rejection, or deferral decision and never places an order.

## Numeric representation

- Monetary amounts are integer USD cents (`u64`), not floating-point values.
- Leverage and collateral ratios are integer basis points (`bps`); 10,000 bps = 1.0x or 100%.
- Price divergence is also expressed in basis points.
- Portfolio leverage comparison and collateral-buffer calculation use `u128` intermediates to avoid integer overflow in multiplication.

The dashboard accepts human-readable USD and decimal ratios, then converts them to integer cents and basis points before sending them to the API.

## Per-user limits

Each user must have an explicitly stored and validated configuration:

- Maximum concurrent positions, hard-capped at 8.
- Maximum notional per position, in cents.
- Maximum total portfolio exposure, in cents.
- Maximum portfolio leverage, in basis points.
- Maximum permitted Bybit-to-GMX price divergence, in basis points.
- Minimum free-collateral buffer, in basis points.

There are no permissive global defaults. The API validates configurations and writes them using a server-only Supabase service-role key. The browser cannot write risk configurations directly.

## Position-cap behavior

If the configured concurrent-position limit is reached, the engine returns `Deferred`. It does not close, reduce, or otherwise interfere with an existing position to make room for a new signal. A new signal may be reconsidered only after an existing position has been independently confirmed closed and the account snapshot refreshed.

## Required production hardening

- Load risk configuration from the authenticated per-user record; never trust limits supplied directly by the browser.
- Derive account equity, collateral, positions, and market verification from trusted GMX reads.
- Add per-market exposure caps, daily loss limits, liquidation-distance checks, funding/fee estimates, stale-data guards, idempotency, and concurrency locks.
- Bind any eventual execution authorization to one user, one wallet, one market, one side, one notional ceiling, one signal ID, a short expiry, and a one-time nonce.
- Sign and verify execution permits between services. The current crate returns an in-process decision only and is not an authorization token.
- Reconcile exchange/on-chain positions before every new approval and after every order status transition.
