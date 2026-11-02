import { createConfig, http } from "wagmi";
import { arbitrum } from "wagmi/chains";
import { injected } from "@wagmi/core";

export const wagmiConfig = createConfig({
  chains: [arbitrum],
  connectors: [injected({ shimDisconnect: true })],
  transports: {
    [arbitrum.id]: http()
  },
  ssr: true
});
