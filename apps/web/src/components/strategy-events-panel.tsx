"use client";

import { useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";

type StrategyEvent = {
  id: string;
  event_type: "ENTER_BUY" | "ENTER_SELL" | "EXIT_BUY" | "EXIT_SELL";
  source_candle_open_time: string;
  reference_price: number | string;
  created_at: string;
  payload?: { origin?: string; is_live_signal?: boolean; execution_eligible?: boolean };
};

function eventLabel(eventType: StrategyEvent["event_type"]) {
  switch (eventType) {
    case "ENTER_BUY": return "ENTRADA · COMPRA";
    case "ENTER_SELL": return "ENTRADA · VENDA";
    case "EXIT_BUY": return "SAÍDA · COMPRA";
    case "EXIT_SELL": return "SAÍDA · VENDA";
  }
}

export function StrategyEventsPanel() {
  const [userId, setUserId] = useState<string | null>(null);
  const [events, setEvents] = useState<StrategyEvent[]>([]);
  const [message, setMessage] = useState("Entre na conta e ative o monitoramento FMT para receber sinais.");

  useEffect(() => {
    if (!supabase) return;
    void supabase.auth.getSession().then(({ data }) => {
      setUserId(data.session?.user.id ?? null);
    });
    const { data } = supabase.auth.onAuthStateChange((_event, session) => {
      setUserId(session?.user.id ?? null);
    });
    return () => data.subscription.unsubscribe();
  }, []);

  useEffect(() => {
    if (!supabase || !userId) {
      setEvents([]);
      setMessage("Entre na conta e ative o monitoramento FMT para receber sinais.");
      return;
    }

    const client = supabase;
    let cancelled = false;
    const channel = client
      .channel(`fmt-events-${userId}`)
      .on(
        "postgres_changes",
        {
          event: "INSERT",
          schema: "public",
          table: "strategy_events",
          filter: `user_id=eq.${userId}`
        },
        (payload) => {
          const incoming = payload.new as StrategyEvent;
          setEvents((current) => [
            incoming,
            ...current.filter((item) => item.id !== incoming.id)
          ].slice(0, 30));
          setMessage("Recebendo eventos FMT em tempo real.");
        }
      )
      .subscribe((status) => {
        if (status === "SUBSCRIBED") setMessage("Canal de sinais FMT conectado.");
        if (status === "CHANNEL_ERROR" || status === "TIMED_OUT") {
          setMessage("Canal Realtime indisponível; confira a publicação e as políticas RLS.");
        }
      });

    async function loadRecent() {
      const { data, error } = await client
        .from("strategy_events")
        .select("id,event_type,source_candle_open_time,reference_price,created_at,payload")
        .eq("user_id", userId)
        .order("created_at", { ascending: false })
        .limit(30);

      if (cancelled) return;
      if (error) {
        setMessage("Não foi possível carregar o histórico de sinais desta conta.");
        return;
      }
      const recent = (data ?? []) as StrategyEvent[];
      setEvents((current) => {
        const merged = new Map<string, StrategyEvent>();
        for (const item of [...current, ...recent]) merged.set(item.id, item);
        return [...merged.values()]
          .sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at))
          .slice(0, 30);
      });
      if (recent.length === 0) setMessage("Nenhum sinal registrado. Ative o monitoramento e aguarde um sinal confirmado.");
    }

    void loadRecent();
    return () => {
      cancelled = true;
      void client.removeChannel(channel);
    };
  }, [userId]);

  return (
    <section className="strategy-events panel">
      <div className="panel-heading">
        <div>
          <div className="eyebrow">SINAIS MULTIUSUÁRIO</div>
          <h2>Eventos FMT em tempo real</h2>
        </div>
        <span className="tag">M15 · BYBIT</span>
      </div>
      <p className="api-status">{message}</p>
      {events.length === 0 ? (
        <div className="empty-events">Nenhum evento disponível para esta conta.</div>
      ) : (
        <div className="event-list">
          {events.map((event) => (
            <div className="event-row" key={event.id}>
              <div>
                <strong>{eventLabel(event.event_type)}</strong>
                <span>{new Date(event.source_candle_open_time).toLocaleString("pt-BR", { timeZone: "America/New_York" })} · {event.payload?.is_live_signal ? "AO VIVO" : "REPLAY"} · SEM AUTORIZAÇÃO</span>
              </div>
              <b>{Number(event.reference_price).toLocaleString("pt-BR", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</b>
            </div>
          ))}
        </div>
      )}
      <p className="muted-copy">Estes são sinais da estratégia, não confirmações de execução ou preço de preenchimento. Nenhuma ordem é enviada.</p>
    </section>
  );
}
