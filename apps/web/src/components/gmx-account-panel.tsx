"use client";

import { useEffect, useState } from "react";
import { useWallet } from "@/lib/wallet-context";
import { supabase } from "@/lib/supabase";

export function GmxAccountPanel() {
  const { address, chainId, isConnected } = useWallet();
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState("Conecte e verifique uma carteira Arbitrum para consultar a conta GMX.");

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
    setSnapshot(null);
    setMessage("Conecte e verifique uma carteira Arbitrum para consultar a conta GMX.");
  }, [accessToken, address, chainId]);

  useEffect(() => {
    function clearWalletSnapshot() {
      setSnapshot(null);
      setMessage("Carteira desvinculada. Verifique uma carteira para consultar novamente.");
    }
    window.addEventListener("fmt-wallet-unlinked", clearWalletSnapshot);
    return () => window.removeEventListener("fmt-wallet-unlinked", clearWalletSnapshot);
  }, []);

  async function loadAccount() {
    if (!isConnected || !address || chainId !== 42161 || !accessToken) return;
    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (!apiUrl) {
      setMessage("API não configurada.");
      return;
    }

    setLoading(true);
    setMessage("Consultando posições, ordens e saldos diretamente na GMX V2...");
    setSnapshot(null);
    try {
      const url = new URL(`${apiUrl.replace(/\/$/, "")}/v1/gmx/account`);
      url.searchParams.set("wallet_address", address);
      const response = await fetch(url, {
        headers: { Authorization: `Bearer ${accessToken}` }
      });
      if (response.status === 403) {
        throw new Error("Esta carteira ainda não foi verificada e associada à sua conta.");
      }
      if (!response.ok) throw new Error(`Consulta GMX falhou (HTTP ${response.status}).`);
      const data = (await response.json()) as unknown;
      setSnapshot(data);
      setMessage("Consulta GMX concluída. Dados somente para leitura.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Não foi possível consultar a GMX.");
    } finally {
      setLoading(false);
    }
  }

  const disabled = !isConnected || !address || chainId !== 42161 || !accessToken || loading;

  return (
    <section className="gmx-account">
      <div className="panel-heading">
        <div>
          <div className="eyebrow">GMX V2 · ARBITRUM ONE</div>
          <h2>Posições, ordens e saldos</h2>
        </div>
        <span className="tag">SOMENTE LEITURA</span>
      </div>
      <p className="muted-copy">A API só consulta o endereço se ele estiver verificado e vinculado à sua conta. Nenhuma aprovação ou ordem é enviada.</p>
      <button className="button primary" onClick={loadAccount} disabled={disabled}>
        {loading ? "CONSULTANDO..." : "CONSULTAR CONTA GMX"}
      </button>
      <p className="api-status">{message}</p>
      {snapshot !== null ? (
        <details className="gmx-json" open>
          <summary>Resposta da conta GMX</summary>
          <pre>{JSON.stringify(snapshot, null, 2)}</pre>
        </details>
      ) : null}
    </section>
  );
}
