"use client";

import { useEffect, useMemo, useState } from "react";
import { supabase } from "@/lib/supabase";

type PaperPosition = {
  id: string;
  side: "BUY" | "SELL";
  entry_price: number | string;
  entry_candle_open_time: string;
  entry_origin: "REST_BACKFILL" | "LIVE_WEBSOCKET";
  exit_price: number | string | null;
  exit_candle_open_time: string | null;
  pnl_bps: number | null;
  status: "OPEN" | "CLOSED";
};

function price(value: number | string | null) {
  if (value === null) return "—";
  return Number(value).toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  });
}

function pnl(value: number | null) {
  if (value === null) return "ABERTA";
  const percent = value / 100;
  return `${percent > 0 ? "+" : ""}${percent.toFixed(2)}%`;
}

export function PaperTradingPanel() {
  const [userId, setUserId] = useState<string | null>(null);
  const [positions, setPositions] = useState<PaperPosition[]>([]);
  const [message, setMessage] = useState("Entre na conta para consultar o histórico paper.");

  useEffect(() => {
    if (!supabase) return;
    void supabase.auth.getSession().then(({ data }) => {
      setUserId(data.session?.user.id ?? null);
    });
    const { data } = supabase.auth.onAuthStateChange((_event, session) => {
      setUserId(session?.user.id ?? null);
      setPositions([]);
    });
    return () => data.subscription.unsubscribe();
  }, []);

  useEffect(() => {
    if (!supabase || !userId) {
      setMessage("Entre na conta para consultar o histórico paper.");
      setPositions([]);
      return;
    }
    const client = supabase;
    let cancelled = false;

    async function load() {
      const { data, error } = await client
        .from("paper_positions")
        .select("id,side,entry_price,entry_candle_open_time,entry_origin,exit_price,exit_candle_open_time,pnl_bps,status")
        .order("created_at", { ascending: false })
        .limit(50);
      if (cancelled) return;
      if (error) {
        setMessage("Não foi possível carregar o ledger paper. Aplique a migration e confira o RLS.");
        return;
      }
      setPositions((data ?? []) as PaperPosition[]);
      setMessage(data?.length
        ? "Paper trading atualizado em tempo real · PnL bruto, sem custos."
        : "Aguardando o primeiro sinal FMT para iniciar o ledger paper.");
    }

    const channel = client
      .channel(`paper-positions-${userId}`)
      .on(
        "postgres_changes",
        {
          event: "*",
          schema: "public",
          table: "paper_positions",
          filter: `user_id=eq.${userId}`
        },
        () => void load()
      )
      .subscribe((status) => {
        if (status === "CHANNEL_ERROR" || status === "TIMED_OUT") {
          setMessage("Realtime do ledger paper indisponível; confira a publicação e as políticas RLS.");
        }
      });

    void load();
    return () => {
      cancelled = true;
      void client.removeChannel(channel);
    };
  }, [userId]);

  const summary = useMemo(() => {
    const open = positions.filter((position) => position.status === "OPEN").length;
    const closed = positions.filter((position) => position.status === "CLOSED");
    const wins = closed.filter((position) => (position.pnl_bps ?? 0) > 0).length;
    const grossPnl = closed.reduce((total, position) => total + (position.pnl_bps ?? 0), 0);
    return { open, closed: closed.length, wins, grossPnl };
  }, [positions]);

  return (
    <section className="paper-panel panel">
      <div className="panel-heading">
        <div>
          <div className="eyebrow">SIMULAÇÃO · SEM ORDENS REAIS</div>
          <h2>Paper trading FMT</h2>
        </div>
        <span className="tag">GROSS PNL</span>
      </div>
      <div className="paper-summary">
        <div><span>ABERTAS</span><strong>{summary.open}</strong></div>
        <div><span>FECHADAS</span><strong>{summary.closed}</strong></div>
        <div><span>POSITIVAS</span><strong>{summary.wins}</strong></div>
        <div><span>PNL BRUTO</span><strong className={summary.grossPnl >= 0 ? "positive" : "negative"}>{pnl(summary.grossPnl)}</strong></div>
      </div>
      <p className="api-status">{message}</p>
      {positions.length ? (
        <div className="paper-table-wrap">
          <table className="paper-table">
            <thead>
              <tr>
                <th>STATUS</th>
                <th>LADO</th>
                <th>ENTRADA</th>
                <th>SAÍDA</th>
                <th>RESULTADO</th>
                <th>ORIGEM</th>
              </tr>
            </thead>
            <tbody>
              {positions.map((position) => (
                <tr key={position.id}>
                  <td><span className={`paper-status ${position.status.toLowerCase()}`}>{position.status}</span></td>
                  <td className={position.side === "BUY" ? "positive" : "negative"}>{position.side}</td>
                  <td>{price(position.entry_price)}</td>
                  <td>{price(position.exit_price)}</td>
                  <td className={position.pnl_bps === null ? "" : position.pnl_bps >= 0 ? "positive" : "negative"}>{pnl(position.pnl_bps)}</td>
                  <td>{position.entry_origin === "LIVE_WEBSOCKET" ? "AO VIVO" : "REPLAY"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
      <p className="muted-copy">Resultado calculado pela variação do preço de referência Bybit. Não inclui taxas, slippage, funding, liquidação ou preço real de execução GMX.</p>
    </section>
  );
}
