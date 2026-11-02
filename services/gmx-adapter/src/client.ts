import { GmxApiSdk } from "@gmx-io/sdk/v2";

export const GMX_ARBITRUM_CHAIN_ID = 42161 as const;

export type WalletAddress = `0x${string}`;

export interface GmxAccountSnapshot {
  address: WalletAddress;
  positions: Awaited<ReturnType<GmxApiSdk["fetchPositionsInfo"]>>;
  orders: Awaited<ReturnType<GmxApiSdk["fetchOrders"]>>;
  balances: Awaited<ReturnType<GmxApiSdk["fetchWalletBalances"]>>;
}

/**
 * Read-only GMX V2 adapter.
 *
 * This service deliberately does not accept private keys, create approvals,
 * activate subaccounts, or submit orders. Trade submission will be added only
 * behind the Rust risk engine and a user-authorized wallet/subaccount signer.
 */
export class GmxReadAdapter {
  private readonly sdk: GmxApiSdk;

  constructor() {
    this.sdk = new GmxApiSdk({ chainId: GMX_ARBITRUM_CHAIN_ID });
  }

  async getMarketCatalog() {
    const [markets, marketInfo, marketConfig, marketValues] = await Promise.all([
      this.sdk.fetchMarkets(),
      this.sdk.fetchMarketsInfo(),
      this.sdk.fetchMarketsConfig(),
      this.sdk.fetchMarketsValues(),
    ]);

    return { chainId: GMX_ARBITRUM_CHAIN_ID, markets, marketInfo, marketConfig, marketValues };
  }

  async getAccountSnapshot(address: WalletAddress): Promise<GmxAccountSnapshot> {
    const [positions, orders, balances] = await Promise.all([
      this.sdk.fetchPositionsInfo({ address, includeRelatedOrders: true }),
      this.sdk.fetchOrders({ address }),
      this.sdk.fetchWalletBalances({ address }),
    ]);

    return { address, positions, orders, balances };
  }

  async getMarketTickers(symbols?: string[]) {
    return this.sdk.fetchMarketsTickers(symbols?.length ? { symbols } : {});
  }
}
