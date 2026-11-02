# Web dashboard

Initial Next.js dashboard shell for FMT QUANTUM.

## Included

- Dark responsive trading-dashboard layout.
- MetaMask/injected-wallet connection through wagmi.
- EIP-191 ownership challenge signing on Arbitrum One; the backend verifies the recovered address and stores the association.
- Supabase email magic-link sign-in.
- API session verification against the Rust `/v1/me` endpoint.
- Per-user risk configuration editor backed by `/v1/risk-config`.
- On-demand read-only GMX account snapshot for verified Arbitrum wallets.
- Arbitrum One network detection and switch action.
- Clear development/offline indicators.
- No trade, approval, deposit, withdrawal, or delegated-key controls.

## Not yet connected

The UI can request GMX account snapshots on demand, but is not connected to Bybit live data or a real-time position stream. The risk settings API is connected, but it does not yet control a live execution path. Connecting a wallet in this shell does not yet prove wallet ownership; a server-verified signature challenge is required before associating a wallet with a user account.

Execution remains disabled.
