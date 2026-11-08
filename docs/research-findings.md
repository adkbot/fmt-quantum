# Pesquisa de estratégias — resultados e como reproduzir

Fonte: candles públicos da Bybit (BTCUSDT linear perpetual, M15, 365 dias, sem lacunas) gravados em
`public.market_candles`. Reprodução: funções do schema `backtest` (sem acesso para anon/authenticated).
Custo de ida e volta usado: 11–12 bps (taxas + derrapagem; é uma suposição, meça o seu).

| Estratégia / variação | Operações | Resultado |
|---|---|---|
| FMT (sweep Pré-Tóquio + VWAP), saídas originais | 323 | bruto ≈ −35 bps no total; líquido de custos fortemente negativo |
| FMT + stop na vela do sweep + alvo 3R | 323 | 26% de acerto, +12 R bruto, −175 R líquido |
| Initial Balance + VWAP + 3R (painel HTML original) | 337 | painel mostrava +815 R; resolvendo vela a vela: ≈ 0 R bruto, −121 a −134 R líquido |
| Seguidor de tendência no cruzamento da VWAP (com parciais) | ~1.200 | bruto ≈ −1 a −3 bps por operação, todas as variações |
| Dual VWAP Sniper (especificação), nota ≥ 80 | 92 | bruto +0,12 R, líquido −0,10 R/operação (t = −0,75); frágil à janela da VWAP rápida |
| Dual VWAP Sniper com R/R ≥ 3 | 35 | líquido +0,15 R/operação, t = 0,6 — sem significância estatística |

Conclusão atual: nenhuma variação mostrou vantagem estatisticamente robusta depois dos custos.
A simulação antiga do painel (`analise.html` original) decidia o resultado na própria vela do sinal
(olhava o futuro) e contava operações sem resolução como vitória; a versão atual resolve nas velas seguintes.

Cuidados: amostras pequenas, uma única janela de mercado, muitas variações testadas (risco de ajuste ao passado).
Antes de qualquer capital real: validação fora da amostra, mais ativos/períodos, custos reais medidos e paper trading.
