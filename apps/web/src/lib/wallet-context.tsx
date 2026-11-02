"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode
} from "react";
import { toHex, type Address } from "viem";

type Eip1193Provider = {
  request(args: { method: string; params?: unknown[] | object }): Promise<unknown>;
  on?: (eventName: string, listener: (...args: unknown[]) => void) => void;
  removeListener?: (eventName: string, listener: (...args: unknown[]) => void) => void;
};

declare global {
  interface Window {
    ethereum?: Eip1193Provider;
  }
}

type WalletContextValue = {
  address?: Address;
  chainId?: number;
  isConnected: boolean;
  isConnecting: boolean;
  isSigning: boolean;
  error: string | null;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  switchChain: (chainId: number) => Promise<void>;
  signMessageAsync: (message: string) => Promise<string>;
};

const WalletContext = createContext<WalletContextValue | null>(null);

function parseChainId(value: unknown): number | undefined {
  if (typeof value !== "string" || !value.startsWith("0x")) return undefined;
  try {
    return Number(BigInt(value));
  } catch {
    return undefined;
  }
}

function errorCode(error: unknown): number | undefined {
  if (typeof error !== "object" || error === null || !("code" in error)) return undefined;
  const code = (error as { code?: unknown }).code;
  return typeof code === "number" ? code : undefined;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Falha na comunicação com a carteira.";
}

function isAddress(value: unknown): value is Address {
  return typeof value === "string" && /^0x[a-fA-F0-9]{40}$/.test(value);
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<Address | undefined>();
  const [chainId, setChainId] = useState<number | undefined>();
  const [isConnecting, setIsConnecting] = useState(false);
  const [isSigning, setIsSigning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const syncWallet = useCallback(async (provider: Eip1193Provider) => {
    const [accounts, chain] = await Promise.all([
      provider.request({ method: "eth_accounts" }),
      provider.request({ method: "eth_chainId" })
    ]);
    const firstAccount = Array.isArray(accounts) ? accounts[0] : undefined;
    setAddress(isAddress(firstAccount) ? firstAccount : undefined);
    setChainId(parseChainId(chain));
  }, []);

  useEffect(() => {
    const provider = window.ethereum;
    if (!provider) return;

    void syncWallet(provider).catch(() => {
      setAddress(undefined);
      setChainId(undefined);
    });

    const onAccountsChanged = (...args: unknown[]) => {
      const accounts = args[0];
      const firstAccount = Array.isArray(accounts) ? accounts[0] : undefined;
      setAddress(isAddress(firstAccount) ? firstAccount : undefined);
    };
    const onChainChanged = (...args: unknown[]) => setChainId(parseChainId(args[0]));
    const onDisconnect = () => {
      setAddress(undefined);
      setChainId(undefined);
    };

    provider.on?.("accountsChanged", onAccountsChanged);
    provider.on?.("chainChanged", onChainChanged);
    provider.on?.("disconnect", onDisconnect);

    return () => {
      provider.removeListener?.("accountsChanged", onAccountsChanged);
      provider.removeListener?.("chainChanged", onChainChanged);
      provider.removeListener?.("disconnect", onDisconnect);
    };
  }, [syncWallet]);

  const connect = useCallback(async () => {
    const provider = window.ethereum;
    if (!provider) {
      const missing = "MetaMask não foi encontrada neste navegador.";
      setError(missing);
      throw new Error(missing);
    }
    setError(null);
    setIsConnecting(true);
    try {
      const accounts = await provider.request({ method: "eth_requestAccounts" });
      const chain = await provider.request({ method: "eth_chainId" });
      const firstAccount = Array.isArray(accounts) ? accounts[0] : undefined;
      if (!isAddress(firstAccount)) throw new Error("A carteira não retornou um endereço válido.");
      setAddress(firstAccount);
      setChainId(parseChainId(chain));
    } catch (caught) {
      setError(errorMessage(caught));
      throw caught;
    } finally {
      setIsConnecting(false);
    }
  }, []);

  const disconnect = useCallback(async () => {
    const provider = window.ethereum;
    setError(null);
    if (!provider) {
      setAddress(undefined);
      setChainId(undefined);
      return;
    }
    try {
      await provider.request({
        method: "wallet_revokePermissions",
        params: [{ eth_accounts: {} }]
      });
      setAddress(undefined);
      setChainId(undefined);
    } catch {
      setError("Este provedor não oferece desconexão programática. Revogue o acesso nas conexões da MetaMask.");
    }
  }, []);

  const switchChain = useCallback(async (nextChainId: number) => {
    const provider = window.ethereum;
    if (!provider) {
      const missing = "MetaMask não foi encontrada.";
      setError(missing);
      throw new Error(missing);
    }
    const chainIdHex = `0x${nextChainId.toString(16)}`;
    setError(null);
    try {
      await provider.request({
        method: "wallet_switchEthereumChain",
        params: [{ chainId: chainIdHex }]
      });
    } catch (caught) {
      if (errorCode(caught) !== 4902 || nextChainId !== 42161) {
        setError(errorMessage(caught));
        throw caught;
      }
      try {
        await provider.request({
          method: "wallet_addEthereumChain",
          params: [{
            chainId: "0xa4b1",
            chainName: "Arbitrum One",
            nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
            rpcUrls: ["https://arb1.arbitrum.io/rpc"],
            blockExplorerUrls: ["https://arbiscan.io"]
          }]
        });
        await provider.request({
          method: "wallet_switchEthereumChain",
          params: [{ chainId: "0xa4b1" }]
        });
      } catch (addError) {
        setError(errorMessage(addError));
        throw addError;
      }
    }
    setChainId(nextChainId);
  }, []);

  const signMessageAsync = useCallback(async (message: string) => {
    const provider = window.ethereum;
    if (!provider || !address) {
      const missing = "Conecte a MetaMask antes de assinar.";
      setError(missing);
      throw new Error(missing);
    }
    setError(null);
    setIsSigning(true);
    try {
      const signature = await provider.request({
        method: "personal_sign",
        params: [toHex(message), address]
      });
      if (typeof signature !== "string" || !signature.startsWith("0x")) {
        throw new Error("A carteira retornou uma assinatura inválida.");
      }
      return signature;
    } catch (caught) {
      setError(errorMessage(caught));
      throw caught;
    } finally {
      setIsSigning(false);
    }
  }, [address]);

  const value = useMemo<WalletContextValue>(() => ({
    address,
    chainId,
    isConnected: Boolean(address),
    isConnecting,
    isSigning,
    error,
    connect,
    disconnect,
    switchChain,
    signMessageAsync
  }), [address, chainId, isConnecting, isSigning, error, connect, disconnect, switchChain, signMessageAsync]);

  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>;
}

export function useWallet() {
  const context = useContext(WalletContext);
  if (!context) throw new Error("useWallet deve ser usado dentro de WalletProvider.");
  return context;
}
