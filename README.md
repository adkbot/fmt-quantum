# FMT QUANTUM

Plataforma multiusuário não custodial para análise FMT e futura execução de estratégias em mercados perpétuos da GMX V2.

## Estado do projeto

**Fase:** fundação e implementação inicial de estratégia, dados, risco, autenticação e painel.  
**Rede inicial:** Arbitrum One.  
**Protocolo inicial:** GMX V2.  
**Carteira inicial:** MetaMask, com prova de propriedade por assinatura EIP-191.  
**Backend:** Rust.  
**Frontend:** Next.js + TypeScript.  
**Persistência:** PostgreSQL / Supabase.  
**Dados de referência:** Bybit BTCUSDT Linear Perpetual M15.

> **Ainda não está pronto para operar com dinheiro real.** Não há envio de ordens, subcontas GMX ou execução financeira implementados. Testes, paridade com Pine, paper trading e revisão de segurança continuam obrigatórios.

## Princípios de segurança

- Cada usuário mantém a custódia da própria carteira.
- Nunca solicitar, armazenar ou registrar chave privada ou frase-semente da carteira principal.
- A assinatura EIP-191 atual comprova propriedade da carteira; não autoriza ordens, aprovações, depósitos, saques ou delegação.
- Autorizações delegadas futuras devem ser específicas, limitadas, revogáveis e temporárias quando o protocolo permitir.
- Nenhuma autorização de negociação pode conceder retirada geral de fundos.
- Valores financeiros do motor de risco usam centavos inteiros e razões em basis points, não ponto flutuante.
- Ao atingir o limite de posições, o sistema aguarda uma posição encerrar; não fecha uma operação existente apenas para abrir outra.
- Segredos ficam em variáveis de ambiente ou gerenciador de segredos, nunca no Git.

## Estrutura atual

```text
fmt-quantum/
├── Cargo.toml
├── Dockerfile.rust
├── docker-compose.yml
├── crates/
│   ├── fmt-strategy/        # Motor inicial FMT M15
│   ├── bybit-feed/          # Feed REST + WebSocket confirmado
│   ├── fmt-risk/            # Limites individuais e cálculos inteiros
│   ├── fmt-api/             # API autenticada, risco e verificação de carteira
│   └── fmt-worker/          # Backfill, stream, candles e eventos por usuário
├── apps/
│   └── web/
│       ├── Dockerfile       # Imagem de produção Next.js
│       └── src/             # Painel, MetaMask, Supabase e gráfico M15
├── services/
│   └── gmx-adapter/
│       ├── Dockerfile       # Serviço privado de leitura GMX
│       └── src/             # SDK GMX V2 e servidor Fastify
├── supabase/
│   └── migrations/          # Esquema multiusuário, RLS e Realtime
├── .github/workflows/       # CI Rust e TypeScript
└── docs/
```

## Próximas etapas

1. Executar CI e corrigir qualquer falha de compilação, tipos ou testes.
2. Validar paridade do motor FMT com o Pine Script original e candles exportados do TradingView.
3. Endurecer o worker: fila durável, checkpoints, alertas, reconexão e replay de eventos.
4. Validar o adaptador GMX somente leitura, completar a validação de mercados ativos e medir continuamente a divergência entre preço Bybit e oráculo GMX.
5. Integrar o motor de risco a snapshots reais de posições, colateral, equity e limites por usuário.
6. Implementar paper trading e backtesting com custos, funding, slippage e regras de saída.
7. Projetar e auditar a autorização limitada de subconta GMX, sem custódia de carteira.
8. Implementar execução e reconciliação on-chain somente após testes, auditoria e autorização explícita.

Consulte [docs/local-setup.md](docs/local-setup.md), [docs/architecture.md](docs/architecture.md), [docs/strategy-spec.md](docs/strategy-spec.md), [docs/bybit-feed.md](docs/bybit-feed.md) [docs/risk-engine.md](docs/risk-engine.md) e [docs/security-review.md](docs/security-review.md).
