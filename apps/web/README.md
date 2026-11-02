# Web dashboard

Initial Next.js dashboard shell for FMT QUANTUM.

## Included

- Dark responsive trading-dashboard layout.
- MetaMask/injected-wallet connection through wagmi.
- Supabase email magic-link sign-in.
- API session verification against the Rust `/v1/me` endpoint.
- Per-user risk configuration editor backed by `/v1/risk-config`.
- Arbitrum One network detection and switch action.
- Clear development/offline indicators.
- No trade, approval, deposit, withdrawal, or delegated-key controls.

## Not yet connected

The UI is not connected to Bybit live data or GMX account reads. The risk settings API is connected, but it does not yet control a live execution path. Connecting a wallet in this shell does not yet prove wallet ownership; a server-verified signature challenge is required before associating a wallet with a user account.

Execution remains disabled.
