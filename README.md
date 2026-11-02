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

## Estrutura planejada

```text
fmt-quantum/
├── apps/
│   └── web/                 # Next.js + TypeScript
├── crates/
│   ├── fmt-strategy/        # Regras FMT e geração de sinais
│   ├── risk-engine/         # Limites, exposição e validação
│   ├── market-data/         # Preços, candles e VWAP
│   ├── execution-core/      # Orquestração segura de execução
│   └── domain/              # Tipos e modelos compartilhados
├── services/
│   └── gmx-adapter/         # Adaptador TypeScript para SDK oficial GMX
├── docs/
├── infra/
└── tests/
```

## Próximas etapas

1. Congelar a especificação da estratégia FMT original.
2. Definir timeframe e fuso horário de referência.
3. Implementar o motor de sinais com testes de paridade.
4. Implementar gestão de risco e reconciliação de posições.
5. Integrar dados e execução GMX V2 por adaptador isolado.
6. Construir painel multiusuário e persistência.
7. Validar em testnet/paper trading antes de qualquer operação real.

Consulte [docs/architecture.md](docs/architecture.md) e [docs/strategy-spec.md](docs/strategy-spec.md).
