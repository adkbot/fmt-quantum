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

- `cargo fmt`, `cargo test`, and `cargo clippy` passed in GitHub Actions after checking out current branch commit `0d19e39` (including the paper ledger worker).
- GMX adapter `npm run typecheck` passed in GitHub Actions on the same validation run.
- Dashboard TypeScript typecheck and Next.js production build passed in GitHub Actions after checking out commit `0d19e39`, including the EIP-1193 wallet migration and paper ledger UI.
- Docker Compose configuration and all three container image builds (Rust API/worker, GMX adapter, Next.js dashboard) passed in GitHub Actions. A fresh PostgreSQL 16 migration-application check has been added and is pending.
## Remaining unverified

- The Supabase migration has not been applied to a live Supabase project.
- The Bybit feed has not been observed running against a configured environment.
- The GMX SDK reads have not been verified against a connected Arbitrum wallet.
- The literal Pine Script source has not yet been located in the conversation or Library; exact parity remains blocked until the user supplies it.
- No TradingView/Pine parity run has been completed using exported reference candles.
- The paper ledger has been implemented but has not yet been validated end-to-end; no paper-trading or backtest report exists.
- Latest lockfile audit after safe overrides: dashboard reported 0 vulnerabilities; GMX adapter retained 9 low-severity transitive advisories in the ethers/elliptic dependency path. No high or moderate advisories remained.
- No independent security audit has been completed.

## Release gate

Do not deploy for real-money use or enable execution until the checks above are completed, failures are fixed, the strategy is proven equivalent to the agreed Pine behavior, GMX market/account reads are reconciled, paper trading is reviewed, and the delegated authorization design passes an independent security review.

**Execution remains disabled.**
