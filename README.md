# FMT QUANTUM

Plataforma multiusuário para análise e execução automatizada de estratégias em mercados perpétuos da GMX V2, com arquitetura não custodial.

## Estado do projeto

**Fase:** fundação e primeiros módulos de estratégia, dados, risco e leitura GMX.  
**Rede inicial:** Arbitrum One.  
**Protocolo inicial:** GMX V2.  
**Carteira inicial:** MetaMask.  
**Backend principal:** Rust.  
**Frontend planejado:** Next.js + TypeScript.  
**Persistência:** PostgreSQL / Supabase.  
**Atualizações em tempo real:** WebSockets.

> Este repositório ainda não representa um sistema pronto para operar com dinheiro real. A integração, os testes, a revisão de segurança e a validação em testnet/paper trading são etapas obrigatórias.

## Princípios de segurança

- Cada usuário mantém a custódia da própria carteira.
- Nunca solicitar, armazenar ou registrar chave privada ou frase-semente da carteira principal.
- Autorizações delegadas devem ser específicas, limitadas, revogáveis e, quando suportado, temporárias.
- Nenhuma autorização de negociação pode conceder retirada geral de fundos.
- O motor de risco valida cada ordem antes de qualquer execução.
- O sistema deve reconciliar posições reais com o estado interno; nunca presumir que uma posição foi encerrada apenas porque uma sessão terminou.
- Segredos ficam em variáveis de ambiente ou gerenciador de segredos, nunca no Git.

## Estrutura atual

```text
fmt-quantum/
├── Cargo.toml
├── crates/
│   ├── fmt-strategy/        # Motor inicial FMT M15
│   ├── bybit-feed/          # Feed REST Bybit V5 BTCUSDT linear
│   ├── fmt-risk/            # Limites e validação por usuário
│   ├── fmt-api/             # API Rust com validação de JWT Supabase
│   └── fmt-worker/          # Backfill REST + WebSocket confirmado + motor FMT
├── services/
│   └── gmx-adapter/         # GMX SDK v2, somente leitura nesta fase
├── supabase/
│   └── migrations/          # Esquema multiusuário e RLS
├── .github/workflows/       # CI Rust
└── docs/
```

## Próximas etapas

1. Concluir testes e paridade do motor FMT com o Pine Script original.
2. Completar WebSocket Bybit, backfill, reconexão e detecção de lacunas.
3. Endurecer o motor de risco e implementar autorização de execução de uso único.
4. Completar leituras GMX e verificação de mercados/oráculos.
5. Implementar autenticação, API Rust, painel Next.js e atualizações WebSocket.
6. Implementar fluxo de subconta GMX com autorização limitada, sem custódia de carteira.
7. Implementar execução, reconciliação on-chain, paper trading e testes de segurança.
8. Só liberar operações reais após auditoria e autorização explícita do usuário.

Consulte [docs/architecture.md](docs/architecture.md), [docs/strategy-spec.md](docs/strategy-spec.md), [docs/bybit-feed.md](docs/bybit-feed.md) e [docs/risk-engine.md](docs/risk-engine.md).
