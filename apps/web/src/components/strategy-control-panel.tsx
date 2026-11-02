"use client";

import { useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";

type StrategyInstance = {
  id: string;
  user_id: string;
  strategy_code: string;
  source_symbol: string;
  is_enabled: boolean;
  created_at: string;
  updated_at: string;
};

export function StrategyControlPanel() {
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("Entre na conta para configurar o monitoramento FMT.");

  useEffect(() => {
    if (!supabase) return;
    void supabase.auth.getSession().then(({ data }) => {
      setAccessToken(data.session?.access_token ?? null);
    });
    const { data } = supabase.auth.onAuthStateChange((_event, session) => {
      setAccessToken(session?.access_token ?? null);
    });
    return () => data.subscription.unsubscribe();
  }, []);

  useEffect(() => {
    if (!accessToken) {
      setEnabled(false);
      setMessage("Entre na conta para configurar o monitoramento FMT.");
      return;
    }
    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (!apiUrl) {
      setMessage("API não configurada.");
      return;
    }

    let cancelled = false;
    async function load() {
      setLoading(true);
      try {
        const response = await fetch(`${apiUrl.replace(/\/$/, "")}/v1/strategy`, {
          headers: { Authorization: `Bearer ${accessToken}` }
        });
        if (!response.ok) throw new Error(`Falha ao carregar estratégia (HTTP ${response.status}).`);
        const rows = (await response.json()) as StrategyInstance[];
        if (!cancelled) {
          setEnabled(rows[0]?.is_enabled ?? false);
          setMessage(rows.length ? "Configuração FMT carregada." : "Monitoramento ainda não configurado.");
        }
      } catch (error) {
        if (!cancelled) setMessage(error instanceof Error ? error.message : "Erro ao carregar estratégia.");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [accessToken]);

  async function toggle() {
    if (!accessToken || saving) return;
    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (!apiUrl) {
      setMessage("API não configurada.");
      return;
    }
    const nextEnabled = !enabled;
    setSaving(true);
    try {
      const response = await fetch(`${apiUrl.replace(/\/$/, "")}/v1/strategy`, {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${accessToken}`,
          "Content-Type": "application/json"
        },
        body: JSON.stringify({ enabled: nextEnabled })
      });
      if (response.status === 409 && nextEnabled) {
        throw new Error("Salve primeiro seus limites individuais de risco antes de ativar o monitoramento.");
      }
      if (!response.ok) throw new Error(`A API recusou a alteração (HTTP ${response.status}).`);
      const saved = (await response.json()) as StrategyInstance;
      setEnabled(saved.is_enabled);
      setMessage(saved.is_enabled
        ? "Monitoramento FMT ativado. Sinais serão registrados; nenhuma ordem será enviada."
        : "Monitoramento FMT desativado.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Não foi possível alterar a estratégia.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="strategy-control">
      <div className="panel-heading">
        <div>
          <div className="eyebrow">MONITORAMENTO</div>
          <h2>FMT Tóquio · BTCUSDT M15</h2>
        </div>
        <span className={enabled ? "tag" : "tag muted-tag"}>{enabled ? "ATIVO" : "INATIVO"}</span>
      </div>
      <p className="muted-copy">Ativar registra os sinais FMT desta conta. Não envia ordens, não aprova tokens e não concede acesso de saque.</p>
      <button className={enabled ? "button secondary" : "button primary"} onClick={toggle} disabled={!accessToken || loading || saving}>
        {loading ? "CARREGANDO..." : saving ? "SALVANDO..." : enabled ? "DESATIVAR MONITORAMENTO" : "ATIVAR MONITORAMENTO"}
      </button>
      <p className="api-status">{message}</p>
    </section>
  );
}
