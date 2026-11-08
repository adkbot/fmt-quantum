"use client";

import { useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";

type Position = {
  symbol: string;
  side: "Buy" | "Sell";
  size: number | null;
  entry: number | null;
  mark: number | null;
  unrealized: number | null;
  liq: number | null;
  leverage: number | null;
  stop_loss: number | null;
  take_profit: number | null;
};

type Snapshot = {
  environment: "testnet" | "production";
  equity: number | string | null;
  available: number | string | null;
  unrealized_pnl: number | string | null;
  daily_realized_pnl: number | string | null;
  margin_mode: string | null;
  positions: Position[] | null;
  error: string | null;
  updated_at: string;
};

function money(value: number | string | null | undefined) {
  if (value === null || value === undefined || value === "") return "—";
  const n = Number(value);
  return Number.isFinite(n) ? n.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 }) : "—";
}

function num(value: number | null | undefined) {
  return value === null || value === undefined || !Number.isFinite(value) || value === 0 ? "—" : value.toLocaleString("en-US", { maximumFractionDigits: 6 });
}

function tone(value: number | string | null | undefined) {
  const n = Number(value);
  return !Number.isFinite(n) || n === 0 ? "" : n > 0 ? "positive" : "negative";
}

export function AccountPanel() {
  const [userId, setUserId] = useState<string | null>(null);
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [message, setMessage] = useState("Entre na conta para ver a conta Bybit.");

  useEffect(() => {
    if (!supabase) return;
    void supabase.auth.getSession().then(({ data }) => setUserId(data.session?.user.id ?? null));
    const { data } = supabase.auth.onAuthStateChange((_e, session) => {
      setUserId(session?.user.id ?? null);
      setSnap(null);
    });
    return () => data.subscription.unsubscribe();
  }, []);

  useEffect(() => {
    if (!supabase || !userId) return;
    const client = supabase;
    let cancelled = false;
    async function load() {
      const { data, error } = await client
        .from("bybit_account_snapshots")
        .select("environment,equity,available,unrealized_pnl,daily_realized_pnl,margin_mode,positions,error,updated_at")
        .eq("user_id", userId)
        .maybeSingle();
      if (cancelled) return;
      if (error) {
        setMessage("Não foi possível carregar a conta. Aplique a migration e confira o RLS.");
        return;
      }
      setSnap((data as Snapshot | null) ?? null);
      setMessage(
        data
          ? "Dados reais lidos da API privada da Bybit (somente leitura), atualizados a cada ~45 s."
          : "Conta Bybit não conectada. No Render, defina BYBIT_API_KEY, BYBIT_API_SECRET e EXECUTION_USER_ID (chaves sem permissão de saque)."
      );
    }
    const channel = client
      .channel(`bybit-account-${userId}`)
      .on("postgres_changes", { event: "*", schema: "public", table: "bybit_account_snapshots", filter: `user_id=eq.${userId}` }, () => void load())
      .subscribe();
    void load();
    return () => {
      cancelled = true;
      void client.removeChannel(channel);
    };
  }, [userId]);

  const positions = snap?.positions ?? [];
  const notIsolated = snap?.margin_mode && snap.margin_mode !== "ISOLATED_MARGIN";

  return (
    <section className="paper-panel panel">
      <div className="panel-heading">
        <div>
          <div className="eyebrow">CONTA BYBIT · DADOS REAIS</div>
          <h2>Saldo, posições e resultado do dia</h2>
        </div>
        <span className="tag">{snap ? (snap.environment === "production" ? "CONTA REAL" : "TESTNET") : "DESCONECTADA"}</span>
      </div>

      <div className="paper-summary">
        <div><span>EQUITY (USDT)</span><strong>{money(snap?.equity)}</strong></div>
        <div><span>DISPONÍVEL</span><strong>{money(snap?.available)}</strong></div>
        <div><span>PNL NÃO REALIZADO</span><strong className={tone(snap?.unrealized_pnl)}>{money(snap?.unrealized_pnl)}</strong></div>
        <div><span>PNL REALIZADO HOJE (UTC)</span><strong className={tone(snap?.daily_realized_pnl)}>{money(snap?.daily_realized_pnl)}</strong></div>
      </div>

      {snap?.error ? <p className="muted-copy negative">Erro na leitura da Bybit: {snap.error}</p> : null}
      {notIsolated ? (
        <p className="muted-copy negative">
          Atenção: a conta está em {snap?.margin_mode}. O executor só opera em margem ISOLADA e vai bloquear ordens até você mudar isso na Bybit.
        </p>
      ) : null}

      <div className="paper-table-wrap">
        <table className="paper-table">
          <thead>
            <tr><th>Ativo</th><th>Lado</th><th>Tamanho</th><th>Entrada</th><th>Marcação</th><th>PnL</th><th>Liquidação</th><th>Alav.</th><th>Stop</th><th>Alvo</th></tr>
          </thead>
          <tbody>
            {positions.length === 0 ? (
              <tr><td colSpan={10}>Nenhuma posição aberta.</td></tr>
            ) : (
              positions.map((p) => (
                <tr key={`${p.symbol}-${p.side}`}>
                  <td>{p.symbol}</td>
                  <td className={p.side === "Buy" ? "positive" : "negative"}>{p.side === "Buy" ? "COMPRA" : "VENDA"}</td>
                  <td>{num(p.size)}</td>
                  <td>{num(p.entry)}</td>
                  <td>{num(p.mark)}</td>
                  <td className={tone(p.unrealized)}>{money(p.unrealized)}</td>
                  <td>{num(p.liq)}</td>
                  <td>{p.leverage ? `${p.leverage}x` : "—"}</td>
                  <td>{num(p.stop_loss)}</td>
                  <td>{num(p.take_profit)}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      <p className="muted-copy">
        {message}
        {snap ? ` Última leitura: ${new Date(snap.updated_at).toLocaleTimeString("pt-BR")}.` : ""}
      </p>
    </section>
  );
}
