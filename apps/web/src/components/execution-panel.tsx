"use client";

import { useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";

type Order = {
  id: string;
  symbol: string;
  side: "BUY" | "SELL";
  environment: "testnet" | "production";
  status: "PENDING" | "BLOCKED" | "SUBMITTED" | "REJECTED" | "FAILED";
  reason: string | null;
  reference_price: number | string | null;
  plan: { qty?: string; stop_loss?: string; take_profit?: string; risk_usd?: number; leverage?: number } | null;
  created_at: string;
};

const STATUS_PT: Record<Order["status"], string> = {
  PENDING: "PENDENTE",
  BLOCKED: "BLOQUEADA",
  SUBMITTED: "ENVIADA",
  REJECTED: "REJEITADA",
  FAILED: "FALHOU"
};

export function ExecutionPanel() {
  const [userId, setUserId] = useState<string | null>(null);
  const [orders, setOrders] = useState<Order[]>([]);
  const [message, setMessage] = useState("Entre na conta para ver as ordens enviadas à Bybit.");

  useEffect(() => {
    if (!supabase) return;
    void supabase.auth.getSession().then(({ data }) => setUserId(data.session?.user.id ?? null));
    const { data } = supabase.auth.onAuthStateChange((_e, session) => {
      setUserId(session?.user.id ?? null);
      setOrders([]);
    });
    return () => data.subscription.unsubscribe();
  }, []);

  useEffect(() => {
    if (!supabase || !userId) return;
    const client = supabase;
    let cancelled = false;
    async function load() {
      const { data, error } = await client
        .from("bybit_orders")
        .select("id,symbol,side,environment,status,reason,reference_price,plan,created_at")
        .order("created_at", { ascending: false })
        .limit(30);
      if (cancelled) return;
      if (error) {
        setMessage("Não foi possível carregar as ordens. Aplique a migration e confira o RLS.");
        return;
      }
      setOrders((data ?? []) as Order[]);
      setMessage(
        data?.length
          ? "Ordens registradas pelo executor em tempo real. Toda ordem tem stop loss e take profit."
          : "Nenhuma ordem enviada. A execução é desligada por padrão e configurada no servidor (Render)."
      );
    }
    const channel = client
      .channel(`bybit-orders-${userId}`)
      .on("postgres_changes", { event: "*", schema: "public", table: "bybit_orders", filter: `user_id=eq.${userId}` }, () => void load())
      .subscribe();
    void load();
    return () => {
      cancelled = true;
      void client.removeChannel(channel);
    };
  }, [userId]);

  return (
    <section className="paper-panel panel">
      <div className="panel-heading">
        <div>
          <div className="eyebrow">EXECUÇÃO BYBIT · ORDENS REAIS REGISTRADAS</div>
          <h2>Ordens enviadas e bloqueadas</h2>
        </div>
        <span className="tag">STOP + ALVO 3R</span>
      </div>
      <div className="paper-table-wrap">
        <table className="paper-table">
          <thead>
            <tr><th>Quando</th><th>Ambiente</th><th>Ativo</th><th>Lado</th><th>Qtd</th><th>Stop</th><th>Alvo</th><th>Risco $</th><th>Status</th><th>Motivo</th></tr>
          </thead>
          <tbody>
            {orders.length === 0 ? (
              <tr><td colSpan={10}>—</td></tr>
            ) : (
              orders.map((o) => (
                <tr key={o.id}>
                  <td>{new Date(o.created_at).toLocaleString("pt-BR", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" })}</td>
                  <td>{o.environment === "production" ? "REAL" : "TESTNET"}</td>
                  <td>{o.symbol}</td>
                  <td className={o.side === "BUY" ? "positive" : "negative"}>{o.side === "BUY" ? "COMPRA" : "VENDA"}</td>
                  <td>{o.plan?.qty ?? "—"}</td>
                  <td>{o.plan?.stop_loss ?? "—"}</td>
                  <td>{o.plan?.take_profit ?? "—"}</td>
                  <td>{o.plan?.risk_usd !== undefined ? o.plan.risk_usd.toFixed(2) : "—"}</td>
                  <td><span className={`paper-status ${o.status === "SUBMITTED" ? "open" : "closed"}`}>{STATUS_PT[o.status]}</span></td>
                  <td>{o.reason ?? "—"}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      <p className="muted-copy">{message}</p>
    </section>
  );
}
