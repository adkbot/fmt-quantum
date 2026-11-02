"use client";

import { useAccount, useConnect, useDisconnect, useSwitchChain } from "wagmi";
import { AuthPanel } from "@/components/auth-panel";
import { RiskConfigPanel } from "@/components/risk-config-panel";
import { arbitrum } from "wagmi/chains";

function shortAddress(address?: string) {
  if (!address) return "";
  return `${address.slice(0, 6)}...${address.slice(-4)}`;
}

export default function HomePage() {
  const { address, chainId, isConnected } = useAccount();
  const { connect, connectors, isPending } = useConnect();
  const { disconnect } = useDisconnect();
  const { switchChain, isPending: isSwitching } = useSwitchChain();
  const wrongNetwork = isConnected && chainId !== arbitrum.id;

  return (
    <main className="shell">
      <header className="topbar">
        <div className="brand">
          <div className="brand-mark">FQ</div>
          <div>
            <div className="brand-name">FMT QUANTUM</div>
            <div className="brand-subtitle">NON-CUSTODIAL TRADING SYSTEM</div>
          </div>
        </div>
        <div className="top-actions">
          <span className="network-pill"><span className="dot amber" /> ARBITRUM ONE</span>
          {isConnected ? (
            <div className="wallet-actions">
              <span className="wallet-address">{shortAddress(address)}</span>
              {wrongNetwork ? (
                <button className="button primary" onClick={() => switchChain?.({ chainId: arbitrum.id })} disabled={isSwitching || !switchChain}>
                  {isSwitching ? "TROCANDO..." : "MUDAR REDE"}
                </button>
              ) : null}
              <button className="button secondary" onClick={() => disconnect()}>DESCONECTAR</button>
            </div>
          ) : (
            <button
              className="button primary"
              disabled={isPending || connectors.length === 0}
              onClick={() => connectors[0] && connect({ connector: connectors[0] })}
            >
              {isPending ? "CONECTANDO..." : "CONECTAR METAMASK"}
            </button>
          )}
        </div>
      </header>

      <section className="welcome">
        <div>
          <div className="eyebrow">PAINEL DE CONTROLE</div>
          <h1>Visão geral do sistema</h1>
          <p>Estratégia FMT · BTCUSDT perpétuo · M15 · Sessão configurada em America/New_York</p>
        </div>
        <div className="system-status"><span className="dot amber" /> MODO DE DESENVOLVIMENTO</div>
      </section>

      <section className="metrics-grid">
        <article className="panel metric">
          <span className="metric-label">FEED BYBIT</span>
          <strong className="metric-value muted">OFFLINE</strong>
          <span className="metric-note">WebSocket ainda não conectado ao painel</span>
        </article>
        <article className="panel metric">
          <span className="metric-label">MOTOR FMT</span>
          <strong className="metric-value">M15</strong>
          <span className="metric-note">Motor Rust em implementação</span>
        </article>
        <article className="panel metric">
          <span className="metric-label">GMX V2</span>
          <strong className="metric-value muted">SOMENTE LEITURA</strong>
          <span className="metric-note">Adaptador inicial para Arbitrum One</span>
        </article>
        <article className="panel metric">
          <span className="metric-label">EXECUÇÃO</span>
          <strong className="metric-value muted">BLOQUEADA</strong>
          <span className="metric-note">Nenhuma ordem real será enviada nesta fase</span>
        </article>
      </section>

      <section className="content-grid">
        <article className="panel strategy-panel">
          <div className="panel-heading">
            <div>
              <div className="eyebrow">ESTRATÉGIA ATIVA</div>
              <h2>FMT Tóquio · BTCUSDT</h2>
            </div>
            <span className="tag">CONFIGURADA</span>
          </div>
          <div className="rule-list">
            <div><span>Mercado de referência</span><b>Bybit Linear Perpetual</b></div>
            <div><span>Timeframe</span><b>M15</b></div>
            <div><span>Pré-sessão</span><b>18:00 – 20:00 NY</b></div>
            <div><span>Sessão</span><b>20:00 – 06:00 NY</b></div>
            <div><span>VWAP</span><b>HLC3 × Volume</b></div>
            <div><span>Confirmação</span><b>Fechamento do candle</b></div>
          </div>
        </article>

        <article className="panel wallet-panel">
          <div className="eyebrow">CARTEIRA</div>
          <h2>{isConnected ? "Carteira conectada" : "Conecte sua carteira"}</h2>
          {isConnected ? (
            <>
              <p className="wallet-full">{address}</p>
              <p className="muted-copy">Conexão detectada no navegador. A propriedade da carteira ainda precisa ser verificada por assinatura antes de habilitar funções da conta.</p>
            </>
          ) : (
            <p className="muted-copy">A carteira principal permanece sob sua custódia. O sistema não solicita nem armazena sua chave privada ou frase-semente.</p>
          )}
          <div className="permission-note">
            <span className="dot green" />
            <span>Sem permissão de saque solicitada</span>
          </div>
          <AuthPanel />
          <RiskConfigPanel />
        </article>
      </section>

      <section className="panel positions-panel">
        <div className="panel-heading">
          <div>
            <div className="eyebrow">CONTA GMX</div>
            <h2>Posições e ordens</h2>
          </div>
          <span className="tag muted-tag">AGUARDANDO BACKEND</span>
        </div>
        <div className="empty-state">
          <div className="empty-icon">↗</div>
          <strong>Dados de conta ainda não carregados</strong>
          <p>O adaptador GMX está preparado para consultas, mas a API autenticada e a leitura conectada à carteira ainda estão em desenvolvimento.</p>
        </div>
      </section>

      <footer>
        <span>FMT QUANTUM · BUILD 0.1.0</span>
        <span>AMBIENTE DE DESENVOLVIMENTO · EXECUÇÃO REAL DESABILITADA</span>
      </footer>
    </main>
  );
}
