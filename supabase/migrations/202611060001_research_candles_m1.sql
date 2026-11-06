-- Candles M1 (dados públicos da Bybit) para pesquisa de estratégias. Somente backend/pesquisa.
create table if not exists public.research_candles_m1 (
  symbol text not null,
  open_time timestamptz not null,
  open numeric(30, 10) not null,
  high numeric(30, 10) not null,
  low numeric(30, 10) not null,
  close numeric(30, 10) not null,
  volume numeric(30, 12) not null default 0 check (volume >= 0),
  primary key (symbol, open_time),
  check (high >= low)
);
alter table public.research_candles_m1 enable row level security;
revoke all on public.research_candles_m1 from anon, authenticated;
