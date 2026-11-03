# Validation status

This file separates code that has been authored from code that has actually been executed.

## Authored

- `Cargo.lock`, `apps/web/package-lock.json`, and `services/gmx-adapter/package-lock.json` are committed; Docker and local setup use lockfile-based installs.
- Rust FMT strategy engine and unit tests for M15 validation, pre-session completeness, entry behavior, and same-candle exit/re-entry sequencing.
- Rust risk engine and unit tests for per-user limits, eight-position deferral, market verification, price divergence, and collateral buffer.
- Bybit V5 REST/WebSocket feed and worker backfill/reconnect path.
- Per-user paper position ledger with gross PnL in basis points; REST-replayed signals are non-authorizing and live signals still have `execution_eligible=false`.
- Dashboard paper ledger with Supabase Realtime updates.
- Authenticated Rust API for Supabase identity, risk settings, strategy activation, wallet signature verification, and GMX read-only proxy routes.
- Fastify GMX V2 read-only adapter.
- Next.js dashboard with Supabase Auth, MetaMask EIP-191 verification, risk/strategy controls, Bybit candle chart, FMT Realtime event list, and GMX account snapshot.
- Supabase migration with RLS, user-scoped tables, and Realtime publication.
- Local setup and Docker Compose scaffolding.

## Checks passed in CI

- `cargo fmt`, `cargo test`, and `cargo clippy` passed in GitHub Actions on commit `0d19e39`; the latest CI run for the current branch is rechecking all jobs.
- GMX adapter `npm run typecheck` passed in GitHub Actions on commit `0d19e39`.
- Dashboard TypeScript typecheck and Next.js production build passed in GitHub Actions on commit `0d19e39`, including the EIP-1193 wallet migration and paper ledger UI.
- Docker Compose configuration, all three container image builds (Rust API/worker, GMX adapter, Next.js dashboard), and application of the Supabase migration against a temporary PostgreSQL 16 instance passed in GitHub Actions on prior commits.
## Remaining unverified

- The Supabase migrations were applied to a live free-tier Supabase project (fmt-quantum, us-east-1) on 2026-11-03; the security advisor reported only the intentional `wallet_challenges` no-policy INFO (server-only table) after the `handle_new_user` EXECUTE revoke. End-to-end auth/Realtime flows are still unverified.
- The Bybit feed has not been observed running against a configured environment.
- The GMX SDK reads have not been verified against a connected Arbitrum wallet.
- Daniel supplied the complete Pine Script v5 source; it is archived at `docs/reference/fmt-tokyo-original.pine`. The statement order has been reconciled in the Rust engine and spec; numeric candle-by-candle parity still requires TradingView-exported OHLCV reference data.
- No TradingView/Pine parity run has been completed using exported reference candles.
- The paper ledger has been implemented but has not yet been validated end-to-end; no paper-trading or backtest report exists.
- Latest lockfile audit after safe overrides: dashboard reported 0 vulnerabilities; GMX adapter retained 9 low-severity transitive advisories in the ethers/elliptic dependency path. No high or moderate advisories remained.
- No independent security audit has been completed.

## Release gate

Do not deploy for real-money use or enable execution until the checks above are completed, failures are fixed, the strategy is proven equivalent to the agreed Pine behavior, GMX market/account reads are reconciled, paper trading is reviewed, and the delegated authorization design passes an independent security review.

**Execution remains disabled.**
