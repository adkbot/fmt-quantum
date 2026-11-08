-- Espelho somente-leitura da conta Bybit do operador (saldo, posições, PnL do dia). Somente o backend escreve.
create table if not exists public.bybit_account_snapshots (
  user_id uuid primary key references auth.users(id) on delete cascade,
  environment text not null check (environment in ('testnet', 'production')),
  equity numeric(30, 8),
  available numeric(30, 8),
  unrealized_pnl numeric(30, 8),
  daily_realized_pnl numeric(30, 8),
  margin_mode text,
  positions jsonb not null default '[]'::jsonb,
  error text,
  updated_at timestamptz not null default now()
);
alter table public.bybit_account_snapshots enable row level security;
create policy "account snapshot owner can read" on public.bybit_account_snapshots
  for select to authenticated using (user_id = auth.uid());
do $$
begin
  if exists (select 1 from pg_publication where pubname = 'supabase_realtime')
     and not exists (select 1 from pg_publication_tables where pubname = 'supabase_realtime'
                     and schemaname = 'public' and tablename = 'bybit_account_snapshots') then
    execute 'alter publication supabase_realtime add table public.bybit_account_snapshots';
  end if;
end;
$$;
