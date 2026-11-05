-- Auditoria das ordens reais/testnet enviadas à Bybit pelo executor (somente backend escreve).
create table if not exists public.bybit_orders (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  symbol text not null,
  side text not null check (side in ('BUY', 'SELL')),
  order_link_id text not null unique,
  environment text not null check (environment in ('testnet', 'production')),
  status text not null default 'PENDING' check (status in ('PENDING', 'BLOCKED', 'SUBMITTED', 'REJECTED', 'FAILED')),
  reason text,
  reference_price numeric(30, 10),
  signal_candle_time timestamptz,
  profile text,
  bybit_order_id text,
  plan jsonb,
  response jsonb,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create index if not exists idx_bybit_orders_user_time on public.bybit_orders(user_id, created_at desc);

drop trigger if exists bybit_orders_set_updated_at on public.bybit_orders;
create trigger bybit_orders_set_updated_at
  before update on public.bybit_orders
  for each row execute function public.set_updated_at();

alter table public.bybit_orders enable row level security;
create policy "bybit order owner can read" on public.bybit_orders
  for select to authenticated using (user_id = auth.uid());

do $$
begin
  if exists (select 1 from pg_publication where pubname = 'supabase_realtime')
     and not exists (select 1 from pg_publication_tables where pubname = 'supabase_realtime'
                     and schemaname = 'public' and tablename = 'bybit_orders') then
    execute 'alter publication supabase_realtime add table public.bybit_orders';
  end if;
end;
$$;
