-- FMT QUANTUM initial multi-user schema.
-- Wallet private keys, seed phrases, and delegated signing keys are NEVER stored here.

create extension if not exists pgcrypto;

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
as $
begin
  insert into public.profiles (id, display_name)
  values (new.id, coalesce(new.raw_user_meta_data ->> 'name', new.raw_user_meta_data ->> 'full_name'))
  on conflict (id) do nothing;
  return new;
end;
$;

drop trigger if exists on_auth_user_created_fmt on auth.users;
create trigger on_auth_user_created_fmt
  after insert on auth.users
  for each row execute procedure public.handle_new_user();

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
  wallet_address text not null check (wallet_address ~ '^0x[a-f0-9]{40}
  user_id uuid primary key references auth.users(id) on delete cascade,
  max_open_positions smallint not null default 1 check (max_open_positions between 1 and 8),
  max_position_notional_cents bigint not null check (max_position_notional_cents > 0),
  max_total_exposure_cents bigint not null check (max_total_exposure_cents > 0),
  max_portfolio_leverage_bps integer not null check (max_portfolio_leverage_bps > 0),
  max_price_divergence_bps integer not null default 30 check (max_price_divergence_bps between 0 and 1000),
  min_free_collateral_bps integer not null default 2000 check (min_free_collateral_bps between 0 and 9999),
  updated_at timestamptz not null default now()
);

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

create or replace function public.set_updated_at()
returns trigger
language plpgsql
set search_path = public
as $
begin
  new.updated_at = now();
  return new;
end;
$;

drop trigger if exists strategy_instances_set_updated_at on public.strategy_instances;
create trigger strategy_instances_set_updated_at
  before update on public.strategy_instances
  for each row execute procedure public.set_updated_at();

create unique index if not exists idx_strategy_instances_user_unique
  on public.strategy_instances(user_id, strategy_code, source_symbol);

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

create index if not exists idx_wallet_connections_user on public.wallet_connections(user_id);
create index if not exists idx_strategy_instances_user on public.strategy_instances(user_id);
create index if not exists idx_strategy_events_user_time on public.strategy_events(user_id, source_candle_open_time desc);
create index if not exists idx_positions_user_observed on public.position_snapshots(user_id, observed_at desc);
create index if not exists idx_execution_attempts_user_time on public.execution_attempts(user_id, created_at desc);
create index if not exists idx_audit_events_user_time on public.audit_events(user_id, created_at desc);

alter table public.profiles enable row level security;
alter table public.wallet_connections enable row level security;
alter table public.wallet_challenges enable row level security;
alter table public.risk_configs enable row level security;
alter table public.strategy_instances enable row level security;
alter table public.strategy_events enable row level security;
alter table public.position_snapshots enable row level security;
alter table public.execution_attempts enable row level security;
alter table public.audit_events enable row level security;

create policy "profile owner can read" on public.profiles
  for select to authenticated using (id = auth.uid());
create policy "profile owner can update" on public.profiles
  for update to authenticated using (id = auth.uid()) with check (id = auth.uid());

-- Wallet challenges are server-only; no authenticated client read/write policy.\n\ncreate policy "wallet owner can read" on public.wallet_connections
  for select to authenticated using (user_id = auth.uid());
create policy "wallet owner can unlink" on public.wallet_connections
  for delete to authenticated using (user_id = auth.uid());

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

-- No authenticated client write policies for wallet verification, events, positions,
-- execution records, or audit events. Those writes belong to trusted server services.
),
  nonce text not null unique,
  message text not null,
  expires_at timestamptz not null,
  used_at timestamptz,
  created_at timestamptz not null default now()
);

create index if not exists idx_wallet_challenges_user_nonce
  on public.wallet_challenges(user_id, nonce);
create index if not exists idx_wallet_challenges_expiry
  on public.wallet_challenges(expires_at);
create index if not exists idx_wallet_challenges_active
  on public.wallet_challenges(user_id, wallet_address, expires_at)
  where used_at is null;

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

create index if not exists idx_wallet_connections_user on public.wallet_connections(user_id);
create index if not exists idx_strategy_instances_user on public.strategy_instances(user_id);
create index if not exists idx_strategy_events_user_time on public.strategy_events(user_id, source_candle_open_time desc);
create index if not exists idx_positions_user_observed on public.position_snapshots(user_id, observed_at desc);
create index if not exists idx_execution_attempts_user_time on public.execution_attempts(user_id, created_at desc);
create index if not exists idx_audit_events_user_time on public.audit_events(user_id, created_at desc);

alter table public.profiles enable row level security;
alter table public.wallet_connections enable row level security;
alter table public.risk_configs enable row level security;
alter table public.strategy_instances enable row level security;
alter table public.strategy_events enable row level security;
alter table public.position_snapshots enable row level security;
alter table public.execution_attempts enable row level security;
alter table public.audit_events enable row level security;

create policy "profile owner can read" on public.profiles
  for select to authenticated using (id = auth.uid());
create policy "profile owner can update" on public.profiles
  for update to authenticated using (id = auth.uid()) with check (id = auth.uid());

create policy "wallet owner can read" on public.wallet_connections
  for select to authenticated using (user_id = auth.uid());
create policy "wallet owner can unlink" on public.wallet_connections
  for delete to authenticated using (user_id = auth.uid());

create policy "risk owner can read" on public.risk_configs
  for select to authenticated using (user_id = auth.uid());
create policy "strategy owner can read" on public.strategy_instances
  for select to authenticated using (user_id = auth.uid());
create policy "strategy owner can insert" on public.strategy_instances
  for insert to authenticated with check (user_id = auth.uid());
create policy "strategy owner can update" on public.strategy_instances
  for update to authenticated using (user_id = auth.uid()) with check (user_id = auth.uid());
create policy "strategy owner can delete" on public.strategy_instances
  for delete to authenticated using (user_id = auth.uid());

create policy "event owner can read" on public.strategy_events
  for select to authenticated using (user_id = auth.uid());
create policy "position owner can read" on public.position_snapshots
  for select to authenticated using (user_id = auth.uid());
create policy "execution owner can read" on public.execution_attempts
  for select to authenticated using (user_id = auth.uid());
create policy "audit owner can read" on public.audit_events
  for select to authenticated using (user_id = auth.uid());

-- No authenticated client write policies for wallet verification, events, positions,
-- execution records, or audit events. Those writes belong to trusted server services.
