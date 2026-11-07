-- Oportunidades agora identificam a estratégia (FMT_TOKYO_M15 ou DUAL_VWAP_SNIPER).
alter table public.scanner_opportunities
  add column if not exists strategy text not null default 'FMT_TOKYO_M15'
  check (strategy in ('FMT_TOKYO_M15', 'DUAL_VWAP_SNIPER'));
alter table public.scanner_opportunities drop constraint if exists scanner_opportunities_symbol_event_type_candle_time_key;
alter table public.scanner_opportunities
  add constraint scanner_opportunities_unique_signal unique (symbol, strategy, event_type, candle_time);
