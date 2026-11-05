"use client";

import { useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";

type Opportunity = {
  id: string;
  symbol: string;
  event_type: "ENTER_BUY" | "ENTER_SELL" | "EXIT_BUY" | "EXIT_SELL";
  side: "BUY" | "SELL";
  candle_time: string;
  reference_price: number | string;
};

type ScanSymbol = { symbol: string; turnover_24h: number | string; rank: number };

function price(value: number | string) {
  const n = Number(value);
  return n.toLocaleString("en-US", { minimumFractionDigits: n < 1 ? 5 : 2, maximumFractionDigits: n < 1 ? 5 : 2 });
}

function label(type: Opportunity["event_type"]) {
  return type.startsWith("ENTER") ? "OPORTUNIDADE" : "SAÍDA";
}

export function ScannerPanel() {
  const [userId, setUserId] = useState<string | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [auto, setAuto] = useState(true);
  const [symbols, setSymbols] = useState<ScanSymbol[]>([]);
  const [items, setItems] = useState<Opportunity[]>([]);
  const [message, setMessage] = useState("Carregando scanner…");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!supabase) return;
    void supabase.auth.getSession().then(({ data }) => setUserId(data.session?.user.id ?? null));
    const { data } = supabase.auth.onAuthStateChange((_e, session) => setUserId(session?.user.id ?? null));
    return () => data.subscription.unsubscribe();
  }, []);

  useEffect(() => {
    if (!supabase) return;
    const client = supabase;
    let cancelled = false;

    async function load() {
      const [{ data: syms }, { data: ops, error }] = await Promise.all([
        client.from("scanner_symbols").select("symbol,turnover_24h,rank").eq("is_active", true).order("rank", { ascending: true }),
        client.from("scanner_opportunities").select("id,symbol,event_type,side,candle_time,reference_price").order("candle_time", { ascending: false }).limit(40)
      ]);
      if (cancelled) return;
      setSymbols((syms ?? []) as ScanSymbol[]);
      setItems((ops ?? []) as Opportunity[]);
      setMessage(
        error
          ? "Não foi possível carregar o scanner. Aplique a migration e confira o RLS."
          : (syms?.length ?? 0) === 0
            ? "Aguardando o worker publicar o universo de ativos (atualiza a cada hora)."
            : `Varrendo ${syms?.length} ativos USDT perpétuos mais líquidos da Bybit a cada vela M15 fechada.`
      );
    }

    const channel = client
      .channel("scanner-opportunities")
      .on("postgres_changes", { event: "INSERT", schema: "public", table: "scanner_opportunities" }, () => void load())
      .subscribe();
    void load();
    return () => {
      cancelled = true;
      void client.removeChannel(channel);
    };
  }, []);

  useEffect(() => {
    if (!supabase || !userId) return;
    void supabase
      .from("scanner_settings")
      .select("enabled,auto_paper_enter")
      .eq("user_id", userId)
      .maybeSingle()
      .then(({ data }) => {
        setEnabled(Boolean(data?.enabled));
        setAuto(data?.auto_paper_enter ?? true);
      });
  }, [userId]);

  async function save(nextEnabled: boolean, nextAuto: boolean) {
    if (!supabase || !userId) return;
    setBusy(true);
    const { error } = await supabase
      .from("scanner_settings")
      .upsert({ user_id: userId, enabled: nextEnabled, auto_paper_enter: nextAuto }, { onConflict: "user_id" });
    setBusy(false);
    if (error) {
      setMessage("Não foi possível salvar. Tente novamente.");
      return;
    }
    setEnabled(nextEnabled);
    setAuto(nextAuto);
  }

  return (
    <section className="paper-panel panel">
      <div className="panel-heading">
        <div>
          <div className="eyebrow">SCANNER MULTIATIVOS · TEMPO REAL</div>
          <h2>Oportunidades FMT em todos os pares</h2>
        </div>
        <span className="tag">{symbols.length} ATIVOS</span>
      </div>

      <div className="scanner-controls">
        <label className="scanner-toggle">
          <input type="checkbox" checked={enabled} disabled={!userId || busy} onChange={(e) => void save(e.target.checked, auto)} />
          <span>Scanner automático na minha conta</span>
        </label>
        <label className="scanner-toggle">
          <input type="checkbox" checked={auto} disabled={!userId || busy} onChange={(e) => void save(enabled, e.target.checked)} />
          <span>Entrar automaticamente (paper) quando houver vaga</span>
        </label>
      </div>
      <p className="muted-copy">
        As entradas respeitam o limite de posições simultâneas e as demais regras salvas em &quot;Limites da sua conta&quot;.
        Sem limites salvos, nenhuma entrada automática é aberta.
      </p>

      <div className="paper-table-wrap">
        <table className="paper-table">
          <thead>
            <tr><th>Ativo</th><th>Tipo</th><th>Lado</th><th>Preço ref.</th><th>Vela (NY)</th></tr>
          </thead>
          <tbody>
            {items.length === 0 ? (
              <tr><td colSpan={5}>Nenhuma oportunidade registrada ainda.</td></tr>
            ) : (
              items.map((o) => (
                <tr key={o.id}>
                  <td>{o.symbol}</td>
                  <td><span className={`paper-status ${o.event_type.startsWith("ENTER") ? "open" : "closed"}`}>{label(o.event_type)}</span></td>
                  <td className={o.side === "BUY" ? "positive" : "negative"}>{o.side === "BUY" ? "COMPRA" : "VENDA"}</td>
                  <td>{price(o.reference_price)}</td>
                  <td>{new Date(o.candle_time).toLocaleString("pt-BR", { timeZone: "America/New_York", day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" })}</td>
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
