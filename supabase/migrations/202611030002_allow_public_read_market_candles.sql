-- market_candles contém apenas OHLCV público da Bybit; leitura liberada também para visitantes não logados.
create policy "anyone can read market candles" on public.market_candles
  for select to anon using (true);
