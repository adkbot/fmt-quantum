# Web dashboard

Initial Next.js dashboard shell for FMT QUANTUM.

## Included

- Dark responsive trading-dashboard layout.
- MetaMask/injected-wallet connection through wagmi.
- Arbitrum One network detection and switch action.
- Clear development/offline indicators.
- No trade, approval, deposit, withdrawal, or delegated-key controls.

## Not yet connected

The UI is not connected to Supabase authentication, the Rust API, Bybit live data, or GMX account reads. Connecting a wallet in this shell does not yet prove wallet ownership; a server-verified signature challenge is required before associating a wallet with a user account.

Execution remains disabled.
