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

**Pendente de confirmação:** manter exatamente o fuso/horários de Nova York do código ou adaptar as janelas para o horário local de Tóquio. Não converter nem alterar horários silenciosamente.

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

## Pontos que ainda exigem confirmação

1. Fuso e horário de sessão: manter exatamente New York do script ou adaptar para horário local de Tóquio.
2. Fonte de preço, candles e volume usada para reproduzir a VWAP e os sinais.
3. Comportamento esperado quando saída e nova entrada aparecem no mesmo candle.

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
