"use client";

import { useEffect, useState } from "react";
import { useAccount, useSignMessage } from "wagmi";
import { arbitrum } from "wagmi/chains";
import { supabase } from "@/lib/supabase";

type ChallengeResponse = {
  wallet_address: string;
  chain_id: number;
  nonce: string;
  message: string;
  expires_at: string;
};

type VerifyResponse = {
  verified: boolean;
  wallet_address: string;
  chain_id: number;
};

export function WalletVerificationPanel() {
  const { address, chainId, isConnected } = useAccount();
  const { signMessageAsync, isPending: isSigning } = useSignMessage();
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [verifiedAddress, setVerifiedAddress] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("Conecte a MetaMask e entre na sua conta para verificar a carteira.");

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
    setVerifiedAddress(null);
  }, [address, chainId]);

  useEffect(() => {
    if (!accessToken || !address || chainId !== arbitrum.id) return;
    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (!apiUrl) return;

    let cancelled = false;
    async function loadVerifiedWallets() {
      try {
        const response = await fetch(`${apiUrl.replace(/\\/$/, "")}/v1/wallets`, {
          headers: { Authorization: `Bearer ${accessToken}` }
        });
        if (!response.ok) return;
        const wallets = (await response.json()) as WalletConnection[];
        const match = wallets.find((wallet) =>
          wallet.chain_id === arbitrum.id &&
          wallet.wallet_address.toLowerCase() === address.toLowerCase()
        );
        if (!cancelled) setVerifiedAddress(match?.wallet_address ?? null);
      } catch {
        // A failed read does not imply wallet verification.
      }
    }
    void loadVerifiedWallets();
    return () => {
      cancelled = true;
    };
  }, [accessToken, address, chainId]);

  async function verifyWallet() {
    if (!address || !isConnected) {
      setMessage("Conecte primeiro sua MetaMask.");
      return;
    }
    if (chainId !== arbitrum.id) {
      setMessage("Troque a rede da carteira para Arbitrum One.");
      return;
    }
    if (!accessToken) {
      setMessage("Entre na conta por e-mail antes de verificar a carteira.");
      return;
    }

    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (!apiUrl) {
      setMessage("API não configurada.");
      return;
    }

    setBusy(true);
    setMessage("Solicitando desafio de assinatura...");
    try {
      const baseUrl = apiUrl.replace(/\/$/, "");
      const challengeResponse = await fetch(`${baseUrl}/v1/wallet/challenge`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${accessToken}`,
          "Content-Type": "application/json"
        },
        body: JSON.stringify({ wallet_address: address, chain_id: arbitrum.id })
      });
      if (!challengeResponse.ok) {
        throw new Error(`Não foi possível criar o desafio (HTTP ${challengeResponse.status}).`);
      }
      const challenge = (await challengeResponse.json()) as ChallengeResponse;
      setMessage("Confirme a assinatura da mensagem na MetaMask. Ela não é uma transação.");
      const signature = await signMessageAsync({ message: challenge.message });

      const verifyResponse = await fetch(`${baseUrl}/v1/wallet/verify`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${accessToken}`,
          "Content-Type": "application/json"
        },
        body: JSON.stringify({
          wallet_address: address,
          chain_id: arbitrum.id,
          nonce: challenge.nonce,
          signature
        })
      });
      if (!verifyResponse.ok) {
        throw new Error(`A verificação da carteira falhou (HTTP ${verifyResponse.status}).`);
      }
      const result = (await verifyResponse.json()) as VerifyResponse;
      if (!result.verified) throw new Error("A API não confirmou a carteira.");
      setVerifiedAddress(result.wallet_address);
      setMessage("Carteira verificada e associada à sua conta.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Verificação cancelada ou indisponível.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="wallet-verification">
      <div className="panel-heading">
        <div>
          <div className="eyebrow">PROPRIEDADE DA CARTEIRA</div>
          <h2>Verificação MetaMask</h2>
        </div>
        <span className={verifiedAddress ? "tag" : "tag muted-tag"}>
          {verifiedAddress ? "VERIFICADA" : "PENDENTE"}
        </span>
      </div>
      <p className="muted-copy">
        A carteira assina um desafio EIP-191 de uso único, válido por 5 minutos. A assinatura comprova propriedade; não aprova tokens, não envia transação e não autoriza negociações ou saques.
      </p>
      {address ? <p className="wallet-full">{address}</p> : null}
      <button
        className="button primary"
        onClick={verifyWallet}
        disabled={!isConnected || !accessToken || busy || isSigning || chainId !== arbitrum.id}
      >
        {busy || isSigning ? "VERIFICANDO..." : "VERIFICAR CARTEIRA"}
      </button>
      <p className="api-status">{message}</p>
    </section>
  );
}
