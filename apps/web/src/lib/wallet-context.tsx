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

function isAddress(value: unknown): value is Address {
  return typeof value === "string" && /^0x[a-fA-F0-9]{40}$/.test(value);
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<Address | undefined>();
  const [chainId, setChainId] = useState<number | undefined>();
  const [isConnecting, setIsConnecting] = useState(false);
  const [isSigning, setIsSigning] = useState(false);

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
    if (!provider) throw new Error("MetaMask não foi encontrada neste navegador.");
    setIsConnecting(true);
    try {
      const accounts = await provider.request({ method: "eth_requestAccounts" });
      const chain = await provider.request({ method: "eth_chainId" });
      const firstAccount = Array.isArray(accounts) ? accounts[0] : undefined;
      if (!isAddress(firstAccount)) throw new Error("A carteira não retornou um endereço válido.");
      setAddress(firstAccount);
      setChainId(parseChainId(chain));
    } finally {
      setIsConnecting(false);
    }
  }, []);

  const disconnect = useCallback(async () => {
    const provider = window.ethereum;
    if (provider) {
      try {
        await provider.request({
          method: "wallet_revokePermissions",
          params: [{ eth_accounts: {} }]
        });
      } catch {
        // Some injected wallets do not implement EIP-2255 permission revocation.
      }
    }
    setAddress(undefined);
    setChainId(undefined);
  }, []);

  const switchChain = useCallback(async (nextChainId: number) => {
    const provider = window.ethereum;
    if (!provider) throw new Error("MetaMask não foi encontrada.");
    await provider.request({
      method: "wallet_switchEthereumChain",
      params: [{ chainId: `0x${nextChainId.toString(16)}` }]
    });
    setChainId(nextChainId);
  }, []);

  const signMessageAsync = useCallback(async (message: string) => {
    const provider = window.ethereum;
    if (!provider || !address) throw new Error("Conecte a MetaMask antes de assinar.");
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
    connect,
    disconnect,
    switchChain,
    signMessageAsync
  }), [address, chainId, isConnecting, isSigning, connect, disconnect, switchChain, signMessageAsync]);

  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>;
}

export function useWallet() {
  const context = useContext(WalletContext);
  if (!context) throw new Error("useWallet deve ser usado dentro de WalletProvider.");
  return context;
}
