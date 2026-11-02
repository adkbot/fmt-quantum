# Delegated execution design for GMX V2

## Decision boundary

The main wallet remains the user's owner account. FMT QUANTUM must never request or store the main-wallet private key or seed phrase.

GMX V2 provides a delegated subaccount path for order management. The owner authorizes a delegated address with an expiry, maximum action count, and optional integration ID. The current same-chain express route is `SubaccountGelatoRelayRouter`; the legacy `SubaccountRouter` is not the order path for new integrations.

Official references:
- https://docs.gmx.io/docs/api/contracts/delegated-trading/
- https://docs.gmx.io/docs/sdk/v2/
- https://docs.gmx.io/docs/sdk/v2/examples/

## Selected custody boundary

The current repository has no delegated signer implementation. The safest next implementation compatible with the no-server-key rule is a **browser-memory-only subaccount signer**:

1. The user connects and verifies their owner wallet.
2. The browser creates or re-derives the SDK-managed subaccount signer using the GMX SDK v2.
3. The user reviews and signs the on-chain subaccount authorization in MetaMask.
4. The authorization is restricted to GMX's delegated order action, an explicit expiry, an explicit action count, and the FMT QUANTUM integration ID.
5. The delegated private key exists only in the browser's memory. It is never sent to the API, Supabase, logs, analytics, or browser persistent storage.
6. The browser signs each GMX express order with the delegated signer and submits it through the GMX API relay.
7. The API/worker continue to validate user settings, monitor strategy events, and reconcile GMX state. They never receive the signing key.

### Consequence

Autonomous order execution is available only while the user's browser session is open and the delegated signer remains in memory. Reloading or closing the browser clears the local signer; the user must reconnect and re-derive it. A backend-managed always-on signer would require a separate custody/key-management decision and security audit. It is intentionally not part of this design.

## Permission boundaries

- The subaccount may create, update, and cancel delegated orders within GMX's supported order flow.
- GMX constrains order outputs to the main account.
- Delegated order flows can use the main account's collateral and WNT within supported order execution.
- Deposits, withdrawals, shifts, and claims are not delegated by this subaccount permission.
- Never request or implement a general withdrawal permission.
- Use the shortest practical expiry and the smallest practical action count. Never silently extend either.
- Provide a visible revoke action and show the on-chain authorization state, expiry, remaining actions, and integration ID.
- A database unlink is not an on-chain revoke. The revoke operation must be an explicit owner-signed GMX action.

## Execution gates still required

Before enabling the delegated order path:

- Exact parity tests against the user's original Pine Script and exported TradingView candles.
- A separate server-issued, idempotent execution intent after risk validation. A live WebSocket signal is not authorization; all current strategy events carry `execution_eligible=false`.
- Per-user risk evaluation using fresh GMX positions, balances, equity, market status, trading capacity, fees, funding, and price divergence.
- A fail-closed idempotency/locking layer for each user and order intent.
- Market-symbol mapping from Bybit BTCUSDT to the exact currently active GMX BTC perpetual market; never infer the GMX symbol or address.
- Order prepare/sign/submit/status lifecycle with transaction hash and on-chain execution verification.
- Reconciliation after browser disconnect, API errors, relay failures, partial fills, and session revocation.
- Testnet/paper trading, independent security review, and explicit user authorization before real-money operation.

Until these gates pass, the application must keep real execution disabled.
