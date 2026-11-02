# Local development setup

This starts the current read-only / signal-monitoring build. It does not enable GMX order execution.

## Prerequisites

- Rust stable toolchain with Cargo
- Node.js 22 and npm
- A Supabase project
- MetaMask configured for Arbitrum One

## 1. Configure Supabase

1. Create a Supabase project.
2. Apply `supabase/migrations/202611020001_initial_multiuser_schema.sql` using the Supabase SQL Editor or Supabase CLI.
3. In Supabase Auth, enable email magic-link sign-in.
4. Add `http://localhost:3000` to the allowed redirect URLs.
5. Confirm the `supabase_realtime` publication contains `market_candles` and `strategy_events` after the migration.

Copy the project URL, anon/public key, and service-role key from Supabase. The service-role key is server-only.

## 2. Configure environment files

At the repository root:

PowerShell (Windows 10):

```powershell
Copy-Item .env.example .env
```

Bash:

```bash
cp .env.example .env
```

In `.env`, set:

- `SUPABASE_URL`
- `SUPABASE_ANON_KEY`
- `SUPABASE_SERVICE_ROLE_KEY`
- `GMX_ADAPTER_INTERNAL_KEY`

Generate an internal key.

PowerShell:

```powershell
$bytes = New-Object byte[] 32
[Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
$internalKey = [BitConverter]::ToString($bytes).Replace("-", "").ToLower()
$internalKey
```

Bash:

```bash
openssl rand -hex 32
```

Use the same `GMX_ADAPTER_INTERNAL_KEY` value for the Rust API and TypeScript adapter. Keep `.env` private.

For the browser app:

PowerShell:

```powershell
Copy-Item apps/web/.env.example apps/web/.env.local
```

Bash:

```bash
cp apps/web/.env.example apps/web/.env.local
```

Set `NEXT_PUBLIC_SUPABASE_URL`, `NEXT_PUBLIC_SUPABASE_ANON_KEY`, and `NEXT_PUBLIC_API_URL=http://localhost:8080` in `apps/web/.env.local`. The Rust API and worker load the root `.env`; the GMX adapter also loads it when started from `services/gmx-adapter`.

## 3. Start the GMX read-only adapter

In terminal 1:

```bash
cd services/gmx-adapter
npm install
npm start
```

It listens on `127.0.0.1:8090` by default. It requires the internal API key and exposes read-only GMX endpoints.

## 4. Start the Rust API

In terminal 2, from the repository root:

```bash
cargo run -p fmt-api
```

The API listens on `127.0.0.1:8080`. Set `CORS_ORIGIN=http://localhost:3000`, `GMX_ADAPTER_URL=http://127.0.0.1:8090`, and the same internal key in the root `.env` environment.

## 5. Start the Bybit worker

In terminal 3, from the repository root:

```bash
cargo run -p fmt-worker
```

The worker reads public Bybit BTCUSDT linear perpetual data. It persists confirmed M15 candles and FMT events to Supabase using the server-side service-role key. It does not submit orders.

## 6. Start the dashboard

In terminal 4:

```bash
cd apps/web
npm install
npm run dev
```

Open `http://localhost:3000`.

Sign in by email, connect MetaMask on Arbitrum One, sign the wallet ownership challenge, save risk limits, then enable FMT monitoring. The BTC chart and signal panel populate only while the worker is running and the Supabase migration is applied.

## 7. Validation commands

From the repository root:

```bash
cargo fmt --all
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets
```

For the GMX adapter:

```bash
cd services/gmx-adapter
npm install
npm run typecheck
```

For the dashboard:

```bash
cd apps/web
npm install
npm run typecheck
npm run build
```

## Safety boundary

- Never paste a private key or seed phrase into any environment file or chat.
- Never put the Supabase service-role key in `NEXT_PUBLIC_*` variables.
- The wallet signature is a proof of address ownership only; it is not an order or token approval.
- GMX adapter routes are read-only. The API has no order submission route.
- Do not enable live trading; it is not implemented.


## Optional: run all services with Docker Compose

After filling the root `.env` with Supabase credentials, the shared internal adapter key, and the three `NEXT_PUBLIC_*` values:

```powershell
docker compose up --build
```

Open `http://localhost:3000`. The dashboard, Rust API, private GMX read-only adapter, and Bybit worker start as separate containers. The GMX adapter is not published to the host; only the API can reach it on the internal Docker network.

To stop the services:

```powershell
docker compose down
```

The Compose setup is an initial local deployment scaffold and has not yet been validated by a successful container build.
