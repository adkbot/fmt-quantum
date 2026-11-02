# Arquitetura inicial

## Objetivo

Construir uma plataforma multiusuário em que cada usuário conecta sua própria carteira e autoriza, de forma limitada, as ações de negociação que o protocolo e a carteira suportarem. A plataforma não assume custódia da carteira principal.

## Componentes

### Frontend — Next.js + TypeScript

- Cadastro e autenticação de usuários.
- Conexão de carteira via MetaMask.
- Exibição clara de rede, endereço conectado e permissões.
- Configuração individual de risco e estratégia.
- Painel de mercados, posições, ordens, histórico e eventos.
- Tela para revogar/desconectar permissões quando suportado.

### Backend — Rust

- API autenticada.
- Motor de estratégia FMT.
- Motor de risco independente do modelo de sinais.
- Controle de estado por usuário.
- Reconciliação periódica com o estado on-chain.
- Auditoria de eventos e trilha de execução.

### Adaptador GMX

O SDK oficial da GMX é TypeScript. A arquitetura prevê um adaptador TypeScript isolado para integração com o SDK, enquanto regras de estratégia, risco e domínio permanecem em Rust. Não presumir que existe SDK Rust oficial.

### Dados

- PostgreSQL/Supabase para usuários, preferências, configurações e histórico.
- WebSockets para atualizações de mercado e estado operacional.
- Dados de preço, candles e volume devem ser identificados por fonte e mercado.
- O preço/volume usados pelo indicador precisam ser compatíveis com a fonte de execução escolhida.

## Fluxo de sinal e execução

1. Coletar e validar dados de mercado.
2. Atualizar candles e VWAP ancorada.
3. Avaliar as regras FMT sem alterar a especificação.
4. Gerar sinal tipado e rastreável.
5. Consultar posições e ordens reais do usuário.
6. Aplicar limites individuais no motor de risco.
7. Validar mercado, tamanho, margem, slippage e permissões.
8. Solicitar execução pelo adaptador GMX.
9. Confirmar o resultado on-chain.
10. Atualizar estado, histórico e painel.

O LLM, se vier a ser usado, não terá autoridade direta para enviar transações nem contornar o motor de risco.

## Regras de segurança não negociáveis

- Não armazenar chave privada ou seed da carteira principal.
- Não solicitar autorização irrestrita de retirada.
- Preferir permissões delegadas específicas, limitadas e revogáveis, conforme os recursos efetivamente suportados.
- Nunca considerar uma autorização disponível antes de verificar a implementação real do protocolo/carteira.
- Não abrir operação se o estado on-chain estiver inconsistente ou indisponível.
- Não apagar posição interna no reset de sessão.
- Limitar exposição por usuário, mercado e total da conta.
- Registrar motivo, parâmetros e resultado de cada decisão.
- Testar falhas de RPC, WebSocket, duplicidade de eventos, reorg e execução parcial.

## Limites de operação

O limite de produto planejado é de até 8 posições simultâneas por usuário, condicionado aos limites individuais de risco e aos mercados ativos e verificados na GMX V2. A lista de moedas de interesse não significa que todos esses ativos estejam necessariamente disponíveis como mercados negociáveis.

## Fases de validação

1. Testes unitários e testes de propriedade.
2. Testes de paridade da estratégia com o Pine Script.
3. Simulação histórica com dados identificados.
4. Paper trading.
5. Testnet, quando aplicável à integração.
6. Revisão de segurança e testes de recuperação.
7. Liberação gradual, somente após aprovação explícita.
