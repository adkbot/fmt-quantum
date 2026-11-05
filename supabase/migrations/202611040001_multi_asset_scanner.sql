-- Scanner multiativos: libera outros símbolos Bybit (linear perpetual USDT) e cria as tabelas do scanner.

alter table public.market_candles drop constraint if exists market_candles_source_symbol_check;
alter table public.strategy_instances drop constraint if exists strategy_instances_source_symbol_check;

alter table public.paper_positions drop constraint if exists paper_positions_entry_origin_check;
alter table public.paper_positions add constraint paper_positions_entry_origin_check
  check (entry_origin in ('REST_BACKFILL', 'LIVE_WEBSOCKET', 'SCANNER'));

-- Universo de ativos varridos (atualizado pelo worker a partir dos tickers públicos da Bybit).
create table if not exists public.scanner_symbols (
  symbol text primary key check (symbol ~ '^[A-Z0-9]{1,20}USDT$'),
  turnover_24h numeric(30, 4) not null default 0,
  last_price numeric(30, 10),
  rank integer not null default 0,
  is_active boolean not null default true,
  updated_at timestamptz not null default now()
);

-- Oportunidades encontradas pela lógica FMT (dados públicos de mercado, não são ordens).
create table if not exists public.scanner_opportunities (
  id uuid primary key default gen_random_uuid(),
  symbol text not null,
  event_type text not null check (event_type in ('ENTER_BUY', 'ENTER_SELL', 'EXIT_BUY', 'EXIT_SELL')),
  side text not null check (side in ('BUY', 'SELL')),
  candle_time timestamptz not null,
  reference_price numeric(30, 10) not null check (reference_price > 0),
  detected_at timestamptz not null default now(),
  payload jsonb not null default '{}'::jsonb,
  unique (symbol, event_type, candle_time)
);
create index if not exists idx_scanner_opportunities_time on public.scanner_opportunities(candle_time desc);

-- Preferências do scanner por usuário.
create table if not exists public.scanner_settings (
  user_id uuid primary key references auth.users(id) on delete cascade,
  enabled boolean not null default false,
  auto_paper_enter boolean not null default true,
  updated_at timestamptz not null default now()
);

drop trigger if exists scanner_settings_set_updated_at on public.scanner_settings;
create trigger scanner_settings_set_updated_at
  before update on public.scanner_settings
  for each row execute function public.set_updated_at();

alter table public.scanner_symbols enable row level security;
alter table public.scanner_opportunities enable row level security;
alter table public.scanner_settings enable row level security;

create policy "anyone can read scanner symbols" on public.scanner_symbols
  for select to anon, authenticated using (true);
create policy "anyone can read scanner opportunities" on public.scanner_opportunities
  for select to anon, authenticated using (true);
create policy "scanner owner can read" on public.scanner_settings
  for select to authenticated using (user_id = auth.uid());
create policy "scanner owner can insert" on public.scanner_settings
  for insert to authenticated with check (user_id = auth.uid());
create policy "scanner owner can update" on public.scanner_settings
  for update to authenticated using (user_id = auth.uid()) with check (user_id = auth.uid());

do $$
begin
  if exists (select 1 from pg_publication where pubname = 'supabase_realtime')
     and not exists (select 1 from pg_publication_tables where pubname = 'supabase_realtime'
                     and schemaname = 'public' and tablename = 'scanner_opportunities') then
    execute 'alter publication supabase_realtime add table public.scanner_opportunities';
  end if;
end;
$$;
