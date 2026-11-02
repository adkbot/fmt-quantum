# Especificação de estratégia — FMT Ásia / Tóquio

## Fonte

Esta especificação registra as regras de FMT descritas pelo usuário e confirmadas durante a configuração do projeto. O usuário forneceu o código literal Pine Script v5 nesta conversa. A cópia de referência está em `docs/reference/fmt-tokyo-original.pine`. O motor Rust atual implementa a sequência central das regras; a paridade numérica candle a candle ainda depende de executar ambos sobre a mesma exportação OHLCV do TradingView.

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

## Condição de compra especificada

Durante a sessão:

- `low <= preLow`
- `close > preLow`
- `close[1] <= close`
- `close >= sessionVwap`

## Condição de venda especificada

Durante a sessão:

- `high >= preHigh`
- `close < preHigh`
- `close[1] >= close`
- `close <= sessionVwap`

## Condições de saída especificadas

Compra:
- `high >= preHigh`, ou
- `close < sessionVwap`.

Venda:
- `low <= preLow`, ou
- `close > sessionVwap`.

## Estado operacional

O indicador mantém `emOperacao` e `tipoOperacao`. Ao iniciar a sessão, reinicia essas variáveis. Em um sistema conectado a uma conta real, esse reset visual jamais pode apagar ou ignorar uma posição existente na GMX. A implementação deve reconciliar o estado on-chain antes de permitir novas ações.

## Sequência exata de estado confirmada no Pine v5

1. No início da sessão Tóquio, o indicador reinicia apenas seu estado visual `emOperacao` e `tipoOperacao`.
2. Calcula as condições de saída com o estado que existia antes do candle atual.
3. Se ocorrer saída, zera o estado.
4. Avalia a compra; se entrar, marca posição comprada.
5. Avalia a venda depois da compra. Se compra e venda forem simultaneamente verdadeiras, a compra tem prioridade porque `not emOperacao` já será falso após a compra.
6. Uma saída pode ser seguida por uma nova entrada no mesmo candle. O motor deve emitir `Exit` antes de `Enter`.
7. `close[1]` é o fechamento do candle anterior na série do gráfico, inclusive o candle 19:45 para a primeira barra de Tóquio.
8. A VWAP incorpora o OHLCV do candle atual antes de avaliar sinais e saídas.

## Guardas já implementadas no motor inicial

- A faixa pré-sessão só é considerada válida quando os oito candles M15 entre 18:00 e 19:45 estão presentes e contíguos.
- O primeiro candle de Tóquio usa o fechamento do candle imediatamente anterior (19:45) para reproduzir `close[1]`.
- A série é rejeitada quando há candles duplicados, fora de ordem ou lacunas M15; é necessário fazer backfill antes de retomar.
- O motor avalia saída antes de entrada no mesmo candle, permitindo dois eventos sequenciais quando ambas as condições se verificam. O Pine original fornecido confirma essa ordem. Não é autorização para enviar duas ordens sem reconciliação de risco.

## Critério de paridade

Antes de habilitar execução, usar o Pine literal já arquivado e obter uma exportação de candles M15 do TradingView para comparar ambos os cálculos. Criar testes de referência e comparar:
- início/fim de sessão;
- máxima e mínima pré-sessão;
- VWAP em cada candle;
- sinais de compra e venda;
- condições de saída;
- transições de estado;
- comportamento em virada de dia e mudanças de horário de verão.\n\nA guarda do motor que exige os oito candles M15 completos e contíguos das 18:00 às 19:45 é uma proteção de integridade de dados: o Pine fornecido não contém essa validação explícita. Ela não altera o cálculo quando o histórico está completo, mas pode suprimir sinais em inicializações com histórico parcial. Essa diferença deve permanecer documentada e testada.

Qualquer diferença em relação ao indicador original deve ser documentada e aprovada antes de ser incorporada.
