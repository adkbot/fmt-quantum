# Validation status

This file separates code that has been authored from code that has actually been executed.

## Authored

- Rust FMT strategy engine and unit tests for M15 validation, pre-session completeness, entry behavior, and same-candle exit/re-entry sequencing.
- Rust risk engine and unit tests for per-user limits, eight-position deferral, market verification, price divergence, and collateral buffer.
- Bybit V5 REST/WebSocket feed and worker backfill/reconnect path.
- Authenticated Rust API for Supabase identity, risk settings, strategy activation, wallet signature verification, and GMX read-only proxy routes.
- Fastify GMX V2 read-only adapter.
- Next.js dashboard with Supabase Auth, MetaMask EIP-191 verification, risk/strategy controls, Bybit candle chart, FMT Realtime event list, and GMX account snapshot.
- Supabase migration with RLS, user-scoped tables, and Realtime publication.
- Local setup and Docker Compose scaffolding.

## Not yet executed or verified

- `cargo fmt`, `cargo test`, and `cargo clippy` have not been run in a Rust toolchain against the current repository head.
- GMX adapter `npm run typecheck` has not been run.
- Dashboard `npm run typecheck` and `npm run build` have not been run.
- Docker images and `docker compose up --build` have not been built.
- Cargo/npm lockfiles have not yet been generated and committed, so dependency versions are not fully reproducible.
- The Supabase migration has not been applied to a live Supabase project.
- The Bybit feed has not been observed running against a configured environment.
- The GMX SDK reads have not been verified against a connected Arbitrum wallet.
- No TradingView/Pine parity run has been completed using exported reference candles.
- No paper-trading or backtest report exists.
- No independent security audit has been completed.

## Release gate

Do not deploy for real-money use or enable execution until the checks above are completed, failures are fixed, the strategy is proven equivalent to the agreed Pine behavior, GMX market/account reads are reconciled, paper trading is reviewed, and the delegated authorization design passes an independent security review.

**Execution remains disabled.**
