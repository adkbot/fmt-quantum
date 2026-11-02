# Especificação de estratégia — FMT Ásia / Tóquio

## Fonte

Indicador Pine Script v5 fornecido pelo usuário. O código original é a referência de comportamento. Esta documentação não autoriza modificar suas regras.

## Sessões configuradas no script

- Fuso horário configurável: `America/New_York`.
- Pré-Tóquio: `1800-2000`.
- Sessão chamada Tóquio: `2000-0600`.
- O uso de `America/New_York` faz o horário acompanhar as mudanças de horário de verão dos EUA. O comentário do script menciona EST, mas o identificador IANA também cobre EDT.

A definição final do fuso e dos horários precisa ser confirmada antes da implementação.

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

A fonte de candles e volume da implementação precisa ser definida e alinhada ao mercado GMX escolhido. Não misturar silenciosamente volume de uma exchange com preço de outra.

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

## Pontos que exigem confirmação

1. Timeframe do gráfico TradingView usado como referência.
2. Fuso e horário de sessão desejados: manter exatamente New York do script ou adaptar para horário local de Tóquio.
3. Se os sinais devem ser confirmados apenas no fechamento do candle ou avaliados intrabar.
4. Fonte de preço, candles e volume usada para reproduzir a VWAP e os sinais.
5. Comportamento esperado quando saída e nova entrada aparecem no mesmo candle.

## Critério de paridade

Antes de habilitar execução, criar testes com candles de referência e comparar:
- início/fim de sessão;
- máxima e mínima pré-sessão;
- VWAP em cada candle;
- sinais de compra e venda;
- condições de saída;
- transições de estado;
- comportamento em virada de dia e mudanças de horário de verão.

Qualquer diferença em relação ao indicador original deve ser documentada e aprovada antes de ser incorporada.
