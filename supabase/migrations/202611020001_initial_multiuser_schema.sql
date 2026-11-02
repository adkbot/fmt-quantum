-- FMT QUANTUM initial multi-user schema.
-- Wallet private keys, seed phrases, and delegated signing keys are NEVER stored here.

create extension if not exists pgcrypto;

-- ---------------------------------------------------------------------------
-- User profile
-- ---------------------------------------------------------------------------

create table if not exists public.profiles (
  id uuid primary key references auth.users(id) on delete cascade,
  display_name text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create or replace function public.handle_new_user()
returns trigger
language plpgsql
security definer
set search_path = public
as $$
begin
  insert into public.profiles (id, display_name)
  values (
    new.id,
    coalesce(new.raw_user_meta_data ->> 'name', new.raw_user_meta_data ->> 'full_name')
  )
  on conflict (id) do nothing;
  return new;
end;
$$;

drop trigger if exists on_auth_user_created_fmt on auth.users;
create trigger on_auth_user_created_fmt
  after insert on auth.users
  for each row execute function public.handle_new_user();

create or replace function public.set_updated_at()
returns trigger
language plpgsql
set search_path = public
as $$
begin
  new.updated_at = now();
  return new;
end;
$$;

drop trigger if exists profiles_set_updated_at on public.profiles;
create trigger profiles_set_updated_at
  before update on public.profiles
  for each row execute function public.set_updated_at();

-- ---------------------------------------------------------------------------
-- Verified wallet associations and one-time signature challenges
-- ---------------------------------------------------------------------------

create table if not exists public.wallet_connections (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  chain_id bigint not null check (chain_id > 0),
  wallet_address text not null check (wallet_address ~ '^0x[a-fA-F0-9]{40}$'),
  verified_at timestamptz not null,
  created_at timestamptz not null default now(),
  unique (user_id, chain_id, wallet_address)
);

create table if not exists public.wallet_challenges (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  chain_id bigint not null check (chain_id = 42161),
  wallet_address text not null check (wallet_address ~ '^0x[a-f0-9]{40}$'),
  nonce text not null unique,
  message text not null,
  expires_at timestamptz not null,
  used_at timestamptz,
  created_at timestamptz not null default now()
);

create index if not exists idx_wallet_connections_user
  on public.wallet_connections(user_id);
create index if not exists idx_wallet_challenges_user_nonce
  on public.wallet_challenges(user_id, nonce);
create index if not exists idx_wallet_challenges_expiry
  on public.wallet_challenges(expires_at);
create index if not exists idx_wallet_challenges_active
  on public.wallet_challenges(user_id, wallet_address, expires_at)
  where used_at is null;

-- ---------------------------------------------------------------------------
-- Per-user risk limits: money is stored in integer cents, ratios in basis points
-- ---------------------------------------------------------------------------

create table if not exists public.risk_configs (
  user_id uuid primary key references auth.users(id) on delete cascade,
  max_open_positions smallint not null default 1 check (max_open_positions between 1 and 8),
  max_position_notional_cents bigint not null check (max_position_notional_cents > 0),
  max_total_exposure_cents bigint not null check (max_total_exposure_cents > 0),
  max_portfolio_leverage_bps integer not null check (max_portfolio_leverage_bps > 0),
  max_price_divergence_bps integer not null default 30 check (max_price_divergence_bps between 0 and 1000),
  min_free_collateral_bps integer not null default 2000 check (min_free_collateral_bps between 0 and 9999),
  updated_at timestamptz not null default now()
);

drop trigger if exists risk_configs_set_updated_at on public.risk_configs;
create trigger risk_configs_set_updated_at
  before update on public.risk_configs
  for each row execute function public.set_updated_at();

-- ---------------------------------------------------------------------------
-- Canonical market candles from the confirmed Bybit linear perpetual feed
-- ---------------------------------------------------------------------------

create table if not exists public.market_candles (
  id bigint generated always as identity primary key,
  source_venue text not null default 'BYBIT' check (source_venue = 'BYBIT'),
  source_category text not null default 'linear' check (source_category = 'linear'),
  source_symbol text not null default 'BTCUSDT' check (source_symbol = 'BTCUSDT'),
  timeframe_minutes smallint not null default 15 check (timeframe_minutes = 15),
  open_time timestamptz not null,
  open numeric(30, 10) not null,
  high numeric(30, 10) not null,
  low numeric(30, 10) not null,
  close numeric(30, 10) not null,
  volume numeric(30, 12) not null check (volume >= 0),
  created_at timestamptz not null default now(),
  unique (source_venue, source_category, source_symbol, timeframe_minutes, open_time),
  check (high >= low),
  check (high >= open and high >= close),
  check (low <= open and low <= close)
);

create index if not exists idx_market_candles_symbol_time
  on public.market_candles(source_symbol, open_time desc);

-- ---------------------------------------------------------------------------
-- User strategy activation. The trusted API is the only client write path.
-- ---------------------------------------------------------------------------

create table if not exists public.strategy_instances (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  strategy_code text not null default 'FMT_TOKYO_M15' check (strategy_code = 'FMT_TOKYO_M15'),
  source_venue text not null default 'BYBIT' check (source_venue = 'BYBIT'),
  source_category text not null default 'linear' check (source_category = 'linear'),
  source_symbol text not null default 'BTCUSDT' check (source_symbol = 'BTCUSDT'),
  timeframe_minutes smallint not null default 15 check (timeframe_minutes = 15),
  session_timezone text not null default 'America/New_York' check (session_timezone = 'America/New_York'),
  pre_session_start time not null default '18:00',
  tokyo_session_start time not null default '20:00',
  tokyo_session_end time not null default '06:00',
  is_enabled boolean not null default false,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create unique index if not exists idx_strategy_instances_user_unique
  on public.strategy_instances(user_id, strategy_code, source_symbol);
create index if not exists idx_strategy_instances_enabled
  on public.strategy_instances(strategy_code, source_symbol)
  where is_enabled = true;

drop trigger if exists strategy_instances_set_updated_at on public.strategy_instances;
create trigger strategy_instances_set_updated_at
  before update on public.strategy_instances
  for each row execute function public.set_updated_at();

-- ---------------------------------------------------------------------------
-- Strategy signals, account snapshots, execution audit, and system audit
-- ---------------------------------------------------------------------------

create table if not exists public.strategy_events (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  strategy_instance_id uuid references public.strategy_instances(id) on delete set null,
  event_type text not null check (event_type in ('ENTER_BUY', 'ENTER_SELL', 'EXIT_BUY', 'EXIT_SELL')),
  source_candle_open_time timestamptz not null,
  reference_price numeric(30, 10) not null,
  payload jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  unique (user_id, strategy_instance_id, source_candle_open_time, event_type)
);

create table if not exists public.position_snapshots (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  chain_id bigint not null,
  market_address text not null check (market_address ~ '^0x[a-fA-F0-9]{40}$'),
  position_key text not null,
  is_long boolean not null,
  size_usd numeric(30, 10) not null,
  collateral_usd numeric(30, 10) not null,
  is_open boolean not null,
  observed_at timestamptz not null,
  payload jsonb not null default '{}'::jsonb,
  unique (user_id, position_key, observed_at)
);

create table if not exists public.execution_attempts (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  strategy_event_id uuid references public.strategy_events(id) on delete set null,
  chain_id bigint not null,
  market_address text not null check (market_address ~ '^0x[a-fA-F0-9]{40}$'),
  request_id text,
  transaction_hash text,
  status text not null check (status in ('BLOCKED', 'PREPARED', 'SUBMITTED', 'CREATED', 'EXECUTED', 'CANCELLED', 'FAILED', 'REVERTED')),
  error_code text,
  error_message text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create unique index if not exists idx_execution_attempts_request_id
  on public.execution_attempts(chain_id, request_id)
  where request_id is not null;
create index if not exists idx_strategy_events_user_time
  on public.strategy_events(user_id, source_candle_open_time desc);
create index if not exists idx_positions_user_observed
  on public.position_snapshots(user_id, observed_at desc);
create index if not exists idx_execution_attempts_user_time
  on public.execution_attempts(user_id, created_at desc);

drop trigger if exists execution_attempts_set_updated_at on public.execution_attempts;
create trigger execution_attempts_set_updated_at
  before update on public.execution_attempts
  for each row execute function public.set_updated_at();

-- ---------------------------------------------------------------------------
-- Per-user paper trading ledger (gross PnL only; no fees/slippage/funding)
-- ---------------------------------------------------------------------------

create table if not exists public.paper_positions (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  strategy_instance_id uuid not null references public.strategy_instances(id) on delete cascade,
  side text not null check (side in ('BUY', 'SELL')),
  entry_price numeric(30, 10) not null check (entry_price > 0),
  entry_candle_open_time timestamptz not null,
  entry_origin text not null check (entry_origin in ('REST_BACKFILL', 'LIVE_WEBSOCKET')),
  exit_price numeric(30, 10),
  exit_candle_open_time timestamptz,
  pnl_bps bigint,
  status text not null default 'OPEN' check (status in ('OPEN', 'CLOSED')),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique (strategy_instance_id, entry_candle_open_time),
  check (
    (status = 'OPEN' and exit_price is null and exit_candle_open_time is null and pnl_bps is null)
    or
    (status = 'CLOSED' and exit_price is not null and exit_candle_open_time is not null and pnl_bps is not null)
  )
);

create unique index if not exists idx_paper_positions_one_open_per_strategy
  on public.paper_positions(strategy_instance_id)
  where status = 'OPEN';

create index if not exists idx_paper_positions_user_created
  on public.paper_positions(user_id, created_at desc);

create trigger set_paper_positions_updated_at
  before update on public.paper_positions
  for each row execute function public.set_updated_at();

create table if not exists public.audit_events (
  id uuid primary key default gen_random_uuid(),
  user_id uuid references auth.users(id) on delete set null,
  actor_type text not null check (actor_type in ('USER', 'SYSTEM', 'RISK_ENGINE', 'EXECUTION_ADAPTER')),
  action text not null,
  entity_type text not null,
  entity_id text,
  details jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now()
);

create index if not exists idx_audit_events_user_time
  on public.audit_events(user_id, created_at desc);

-- ---------------------------------------------------------------------------
-- Row-level security
-- ---------------------------------------------------------------------------

alter table public.profiles enable row level security;
alter table public.market_candles enable row level security;
alter table public.wallet_connections enable row level security;
alter table public.wallet_challenges enable row level security;
alter table public.risk_configs enable row level security;
alter table public.strategy_instances enable row level security;
alter table public.strategy_events enable row level security;
alter table public.paper_positions enable row level security;
alter table public.position_snapshots enable row level security;
alter table public.execution_attempts enable row level security;
alter table public.audit_events enable row level security;

create policy "authenticated users can read market candles" on public.market_candles
  for select to authenticated using (true);

create policy "profile owner can read" on public.profiles
  for select to authenticated using (id = auth.uid());
create policy "profile owner can update" on public.profiles
  for update to authenticated using (id = auth.uid()) with check (id = auth.uid());

create policy "wallet owner can read" on public.wallet_connections
  for select to authenticated using (user_id = auth.uid());
create policy "wallet owner can unlink" on public.wallet_connections
  for delete to authenticated using (user_id = auth.uid());

-- wallet_challenges has no authenticated-client policies: server only.
create policy "risk owner can read" on public.risk_configs
  for select to authenticated using (user_id = auth.uid());
create policy "strategy owner can read" on public.strategy_instances
  for select to authenticated using (user_id = auth.uid());
create policy "event owner can read" on public.strategy_events
  for select to authenticated using (user_id = auth.uid());
create policy "position owner can read" on public.position_snapshots
  for select to authenticated using (user_id = auth.uid());
create policy "execution owner can read" on public.execution_attempts
  for select to authenticated using (user_id = auth.uid());
create policy "audit owner can read" on public.audit_events
  for select to authenticated using (user_id = auth.uid());

-- No authenticated-client write policies for wallet verification, risk limits,
-- strategy activation, events, positions, execution records, or audit events.
-- Trusted server services use the Supabase service-role key and must scope every
-- write to the authenticated user ID established by Supabase Auth.


-- Publish strategy signal inserts for authenticated, RLS-scoped dashboard updates.
do $$
begin
  if exists (select 1 from pg_publication where pubname = 'supabase_realtime')
     and not exists (
       select 1
       from pg_publication_tables
       where pubname = 'supabase_realtime'
         and schemaname = 'public'
         and tablename = 'strategy_events'
     ) then
    execute 'alter publication supabase_realtime add table public.strategy_events';
  end if;
  if exists (select 1 from pg_publication where pubname = 'supabase_realtime')
     and not exists (
       select 1 from pg_publication_tables
       where pubname = 'supabase_realtime'
         and schemaname = 'public'
         and tablename = 'market_candles'
     ) then
    execute 'alter publication supabase_realtime add table public.market_candles';
  end if;
  if exists (select 1 from pg_publication where pubname = 'supabase_realtime')
     and not exists (
       select 1 from pg_publication_tables
       where pubname = 'supabase_realtime'
         and schemaname = 'public'
         and tablename = 'paper_positions'
     ) then
    execute 'alter publication supabase_realtime add table public.paper_positions';
  end if;
end;
$$;
