"use client";

import { useWallet } from "@/lib/wallet-context";
import { AuthPanel } from "@/components/auth-panel";
import { RiskConfigPanel } from "@/components/risk-config-panel";
import { WalletVerificationPanel } from "@/components/wallet-verification-panel";
import { GmxAccountPanel } from "@/components/gmx-account-panel";
import { StrategyControlPanel } from "@/components/strategy-control-panel";
import { StrategyEventsPanel } from "@/components/strategy-events-panel";
import { PaperTradingPanel } from "@/components/paper-trading-panel";
import { BtcMarketChart } from "@/components/btc-market-chart";

function shortAddress(address?: string) {
  if (!address) return "";
  return `${address.slice(0, 6)}...${address.slice(-4)}`;
}

export default function HomePage() {
  const { address, chainId, isConnected, isConnecting, connect, disconnect, switchChain, error: walletError } = useWallet();
  const wrongNetwork = isConnected && chainId !== 42161;

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
                <button className="button primary" onClick={() => void switchChain(42161).catch(() => {})} disabled={false}>
                  {"MUDAR REDE"}
                </button>
              ) : null}
              <button className="button secondary" onClick={() => disconnect()}>DESCONECTAR</button>
            </div>
          ) : (
            <button
              className="button primary"
              disabled={isConnecting}
              onClick={() => void connect().catch(() => {})}
            >
              {isConnecting ? "CONECTANDO..." : "CONECTAR METAMASK"}
            </button>
          )}
        </div>
      </header>

      {walletError ? <p className="api-status wallet-error">{walletError}</p> : null}

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
          <strong className="metric-value">WORKER</strong>
          <span className="metric-note">REST + WS implementado; status de execução não monitorado</span>
        </article>
        <article className="panel metric">
          <span className="metric-label">MOTOR FMT</span>
          <strong className="metric-value">RUST M15</strong>
          <span className="metric-note">Regras traduzidas; paridade com Pine ainda pendente</span>
        </article>
        <article className="panel metric">
          <span className="metric-label">GMX V2</span>
          <strong className="metric-value muted">SOMENTE LEITURA</strong>
          <span className="metric-note">Snapshot sob demanda de carteira verificada</span>
        </article>
        <article className="panel metric">
          <span className="metric-label">EXECUÇÃO</span>
          <strong className="metric-value muted">BLOQUEADA</strong>
          <span className="metric-note">Nenhuma ordem real será enviada nesta fase</span>
        </article>
      </section>

      <BtcMarketChart />

      <section className="content-grid">
        <article className="panel strategy-panel">
          <div className="panel-heading">
            <div>
              <div className="eyebrow">MOTOR DE ESTRATÉGIA</div>
              <h2>FMT Tóquio · BTCUSDT</h2>
            </div>
            <span className="tag">FMT M15</span>
          </div>
          <div className="rule-list">
            <div><span>Mercado de referência</span><b>Bybit Linear Perpetual</b></div>
            <div><span>Timeframe</span><b>M15</b></div>
            <div><span>Pré-sessão</span><b>18:00 – 20:00 NY</b></div>
            <div><span>Sessão</span><b>20:00 – 06:00 NY</b></div>
            <div><span>VWAP</span><b>HLC3 × Volume</b></div>
            <div><span>Confirmação</span><b>Fechamento do candle</b></div>
          </div>
          <StrategyControlPanel />
        </article>

        <article className="panel wallet-panel">
          <div className="eyebrow">CARTEIRA</div>
          <h2>{isConnected ? "Carteira conectada" : "Conecte sua carteira"}</h2>
          {isConnected ? (
            <>
              <p className="wallet-full">{address}</p>
              <p className="muted-copy">Conexão detectada no navegador. Use o painel de verificação EIP-191 abaixo para associar esta carteira à sua conta.</p>
            </>
          ) : (
            <p className="muted-copy">A carteira principal permanece sob sua custódia. O sistema não solicita nem armazena sua chave privada ou frase-semente.</p>
          )}
          <div className="permission-note">
            <span className="dot green" />
            <span>Sem permissão de saque solicitada</span>
          </div>
          <AuthPanel />
          <WalletVerificationPanel />
          <RiskConfigPanel />
        </article>
      </section>

      <GmxAccountPanel />
      <StrategyEventsPanel />
      <PaperTradingPanel />

      <footer>
        <span>FMT QUANTUM · BUILD 0.1.0</span>
        <span>AMBIENTE DE DESENVOLVIMENTO · EXECUÇÃO REAL DESABILITADA</span>
      </footer>
    </main>
  );
}
