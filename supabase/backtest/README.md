# Backtest FMT (SQL)

`backtest.fmt_run(p_from, p_to)` é uma porta em PL/pgSQL do motor `fmt-strategy` (Rust) e roda
sobre `public.market_candles` (BTCUSDT M15). Fica no schema `backtest`, sem permissão para
`anon`/`authenticated` (não é exposto pela API REST). Use pelo SQL Editor ou pelo MCP do Supabase.

Entrada e saída no fechamento do candle, como o motor Rust. Posições sem saída ao fim da sessão
são marcadas a mercado no último fechamento (`SESSION_END_MTM`).

O histórico é carregado pelo worker (`HISTORY_DAYS`, padrão 365, máximo 400).
