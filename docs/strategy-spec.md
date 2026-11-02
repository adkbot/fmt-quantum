# Especificação de estratégia — FMT Ásia / Tóquio

## Fonte

Indicador Pine Script v5 fornecido pelo usuário. O código original é a referência de comportamento. Esta documentação não autoriza modificar suas regras.

## Timeframe de referência confirmado

- **M15 (15 minutos)** é o timeframe oficial informado pelo usuário.
- A implementação deverá calcular sessões, níveis, VWAP, sinais e saídas sobre candles M15.
- Como padrão inicial de segurança, os sinais serão confirmados no fechamento do candle M15. Não haverá execução intrabar sem aprovação explícita do usuário.
- O candle anterior usado em `close[1]` será o candle M15 imediatamente anterior na série de referência.

## Sessões configuradas no script

- Fuso horário configurável: `America/New_York`.
- Pré-Tóquio: `1800-2000`.
- Sessão chamada Tóquio: `2000-0600`.
- O uso de `America/New_York` faz o horário acompanhar as mudanças de horário de verão dos EUA. O comentário do script menciona EST, mas o identificador IANA também cobre EDT.

**Confirmado pelo usuário:** manter exatamente o fuso e as janelas do código original. A sessão é a sessão asiática/Tóquio identificada pelo indicador, mas seus horários são definidos no fuso `America/New_York`; não converter para o relógio local de Tóquio.

## Faixa pré-sessão

Durante a janela pré-Tóquio, o indicador calcula:

- `preHigh`: maior máxima observada.
- `preLow`: menor mínima observada.

No início da sessão seguinte, desenha as linhas desses níveis e as estende durante a sessão.

## VWAP ancorada

A acumulação começa no início da janela pré-Tóquio e continua durante a sessão:

- Fonte: `hlc3`.
- Peso: `volume`.
- VWAP: soma de `hlc3 * volume` dividida pela soma de volume.

A estratégia precisa de candles OHLCV para reproduzir a VWAP. A fonte de referência informada pelo usuário é **`BYBIT:BTCUSDT` no TradingView**, timeframe M15. A API oficial de candles GMX e o método `fetchOhlcv()` do SDK v2 documentam OHLC, mas não volume. Portanto, os cálculos de faixa, sinais e VWAP devem usar candles OHLCV de Bybit correspondentes exatamente ao símbolo confirmado, preservando a mesma fonte para todo o cálculo. A GMX será consultada separadamente para validar preço/oráculo, mercado ativo e execução. Diferenças entre as fontes devem ser medidas e limitadas, não ocultadas. O usuário confirmou que o gráfico é do mercado **perpétuo**. A integração deve usar o mercado linear USDT da Bybit (categoria `linear`) para `BTCUSDT`, intervalo de 15 minutos, e validar que o instrumento está ativo antes de consumir os candles.

## Condição original de compra

Durante a sessão:

- `low <= preLow`
- `close > preLow`
- `close[1] <= close`
- `close >= sessionVwap`

## Condição original de venda

Durante a sessão:

- `high >= preHigh`
- `close < preHigh`
- `close[1] >= close`
- `close <= sessionVwap`

## Saídas originais

Compra:
- `high >= preHigh`, ou
- `close < sessionVwap`.

Venda:
- `low <= preLow`, ou
- `close > sessionVwap`.

## Estado operacional

O indicador mantém `emOperacao` e `tipoOperacao`. Ao iniciar a sessão, reinicia essas variáveis. Em um sistema conectado a uma conta real, esse reset visual jamais pode apagar ou ignorar uma posição existente na GMX. A implementação deve reconciliar o estado on-chain antes de permitir novas ações.

## Pontos que ainda exigem confirmação

1. Comportamento esperado quando saída e nova entrada aparecem no mesmo candle.

## Critério de paridade

Antes de habilitar execução, criar testes com candles M15 de referência e comparar:
- início/fim de sessão;
- máxima e mínima pré-sessão;
- VWAP em cada candle;
- sinais de compra e venda;
- condições de saída;
- transições de estado;
- comportamento em virada de dia e mudanças de horário de verão.

Qualquer diferença em relação ao indicador original deve ser documentada e aprovada antes de ser incorporada.
