"use client";

import { FormEvent, useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";

type RiskConfig = {
  max_open_positions: number;
  max_position_notional_usd: number;
  max_total_exposure_usd: number;
  max_portfolio_leverage: number;
  max_price_divergence_bps: number;
  min_free_collateral_ratio: number;
};

type RiskForm = Record<keyof RiskConfig, string>;

const emptyForm: RiskForm = {
  max_open_positions: "",
  max_position_notional_usd: "",
  max_total_exposure_usd: "",
  max_portfolio_leverage: "",
  max_price_divergence_bps: "",
  min_free_collateral_ratio: ""
};

function toForm(config: RiskConfig): RiskForm {
  return {
    max_open_positions: String(config.max_open_positions),
    max_position_notional_usd: String(config.max_position_notional_usd),
    max_total_exposure_usd: String(config.max_total_exposure_usd),
    max_portfolio_leverage: String(config.max_portfolio_leverage),
    max_price_divergence_bps: String(config.max_price_divergence_bps),
    min_free_collateral_ratio: String(config.min_free_collateral_ratio)
  };
}

export function RiskConfigPanel() {
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [form, setForm] = useState<RiskForm>(emptyForm);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("Entre na conta para carregar seus limites individuais.");

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
      setForm(emptyForm);
      setMessage("Entre na conta para carregar seus limites individuais.");
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
        const response = await fetch(`${apiUrl.replace(/\/$/, "")}/v1/risk-config`, {
          headers: { Authorization: `Bearer ${accessToken}` }
        });
        if (response.status === 404) {
          if (!cancelled) {
            setForm(emptyForm);
            setMessage("Ainda não há limites salvos. Preencha e salve sua configuração.");
          }
          return;
        }
        if (!response.ok) throw new Error(`Falha ao carregar limites (HTTP ${response.status}).`);
        const config = (await response.json()) as RiskConfig;
        if (!cancelled) {
          setForm(toForm(config));
          setMessage("Limites individuais carregados da sua conta.");
        }
      } catch (error) {
        if (!cancelled) setMessage(error instanceof Error ? error.message : "Erro ao carregar limites.");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [accessToken]);

  function updateField(field: keyof RiskConfig, value: string) {
    setForm((current) => ({ ...current, [field]: value }));
  }

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!accessToken) return;

    const config: RiskConfig = {
      max_open_positions: Number(form.max_open_positions),
      max_position_notional_usd: Number(form.max_position_notional_usd),
      max_total_exposure_usd: Number(form.max_total_exposure_usd),
      max_portfolio_leverage: Number(form.max_portfolio_leverage),
      max_price_divergence_bps: Number(form.max_price_divergence_bps),
      min_free_collateral_ratio: Number(form.min_free_collateral_ratio)
    };

    if (
      !Number.isInteger(config.max_open_positions) ||
      config.max_open_positions < 1 ||
      config.max_open_positions > 8 ||
      !Number.isFinite(config.max_position_notional_usd) ||
      config.max_position_notional_usd <= 0 ||
      !Number.isFinite(config.max_total_exposure_usd) ||
      config.max_total_exposure_usd <= 0 ||
      !Number.isFinite(config.max_portfolio_leverage) ||
      config.max_portfolio_leverage <= 0 ||
      !Number.isInteger(config.max_price_divergence_bps) ||
      config.max_price_divergence_bps < 0 ||
      config.max_price_divergence_bps > 1000 ||
      !Number.isFinite(config.min_free_collateral_ratio) ||
      config.min_free_collateral_ratio < 0 ||
      config.min_free_collateral_ratio >= 1
    ) {
      setMessage("Confira os valores. Posições: 1–8; divergência: 0–1000 bps; reserva livre: 0 a menos de 1.");
      return;
    }

    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (!apiUrl) {
      setMessage("API não configurada.");
      return;
    }

    setSaving(true);
    try {
      const response = await fetch(`${apiUrl.replace(/\/$/, "")}/v1/risk-config`, {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${accessToken}`,
          "Content-Type": "application/json"
        },
        body: JSON.stringify(config)
      });
      if (!response.ok) throw new Error(`A API recusou a configuração (HTTP ${response.status}).`);
      setForm(toForm(config));
      setMessage("Limites salvos e validados pelo backend.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Não foi possível salvar os limites.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="risk-config">
      <div className="panel-heading">
        <div>
          <div className="eyebrow">CONTROLE DE RISCO</div>
          <h2>Limites da sua conta</h2>
        </div>
        <span className="tag">MÁX. 8 POSIÇÕES</span>
      </div>
      <p className="muted-copy">Os limites são individuais e validados no backend. Ao atingir o limite de posições, o sistema aguarda uma posição encerrar; não fecha uma operação boa para abrir outra.</p>
      <form className="risk-form" onSubmit={save}>
        <label>
          <span>Posições simultâneas (1–8)</span>
          <input type="number" min="1" max="8" step="1" value={form.max_open_positions} onChange={(event) => updateField("max_open_positions", event.target.value)} required disabled={!accessToken || loading || saving} />
        </label>
        <label>
          <span>Notional máximo por posição (USD)</span>
          <input type="number" min="0.01" step="0.01" value={form.max_position_notional_usd} onChange={(event) => updateField("max_position_notional_usd", event.target.value)} required disabled={!accessToken || loading || saving} />
        </label>
        <label>
          <span>Exposição total máxima (USD)</span>
          <input type="number" min="0.01" step="0.01" value={form.max_total_exposure_usd} onChange={(event) => updateField("max_total_exposure_usd", event.target.value)} required disabled={!accessToken || loading || saving} />
        </label>
        <label>
          <span>Alavancagem máxima da carteira</span>
          <input type="number" min="0.01" step="0.01" value={form.max_portfolio_leverage} onChange={(event) => updateField("max_portfolio_leverage", event.target.value)} required disabled={!accessToken || loading || saving} />
        </label>
        <label>
          <span>Divergência máxima de preço (bps)</span>
          <input type="number" min="0" max="1000" step="1" value={form.max_price_divergence_bps} onChange={(event) => updateField("max_price_divergence_bps", event.target.value)} required disabled={!accessToken || loading || saving} />
        </label>
        <label>
          <span>Reserva mínima de colateral (0–0,99)</span>
          <input type="number" min="0" max="0.99" step="0.01" value={form.min_free_collateral_ratio} onChange={(event) => updateField("min_free_collateral_ratio", event.target.value)} required disabled={!accessToken || loading || saving} />
        </label>
        <button className="button primary" type="submit" disabled={!accessToken || loading || saving}>
          {loading ? "CARREGANDO..." : saving ? "SALVANDO..." : "SALVAR LIMITES"}
        </button>
      </form>
      <p className="api-status">{message}</p>
    </section>
  );
}
