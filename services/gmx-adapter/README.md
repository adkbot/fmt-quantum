# GMX V2 adapter

This isolated TypeScript service uses the official `@gmx-io/sdk/v2` client for Arbitrum One (chain ID 42161). It is kept separate because GMX's supported SDK is TypeScript while the strategy/risk core is Rust.

## Implemented in this first slice

- Read GMX market catalog, market info, config, and values.
- Read a connected wallet's positions, active orders, and wallet balances.
- Read market tickers.
- No private key or seed phrase is accepted or persisted.

## Not implemented / intentionally disabled

- Token approvals.
- Subaccount generation or activation.
- Order preparation, signing, and submission.
- Position edits, closes, collateral movement, or withdrawals.
- Any unattended trading.

## Security contract for future execution

1. The user's main wallet remains in the user's custody and signs the explicit subaccount authorization.
2. If one-click trading is enabled, use GMX's scoped subaccount order-action authorization with a short expiry and conservative action count. Do not request or implement general withdrawal permissions.
3. Keep subaccount signing material outside the database and outside logs; use a dedicated secret boundary if a delegated signer is eventually introduced.
4. Every order must carry a one-time, authenticated execution permit produced after the Rust risk engine validates the signal, account limits, market status, current positions, price divergence, and available capacity.
5. Verify GMX order status and on-chain execution evidence; a relay acceptance is not proof of execution.
6. No live order flow is enabled by this adapter.

Official references: [GMX SDK v2 overview](https://docs.gmx.io/docs/sdk/overview/), [SDK v2 getting started](https://docs.gmx.io/docs/sdk/v2/), and [order examples](https://docs.gmx.io/docs/sdk/v2/examples/).
