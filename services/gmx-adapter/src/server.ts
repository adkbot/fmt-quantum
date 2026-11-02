import Fastify, { type FastifyReply } from "fastify";
import dotenv from "dotenv";

dotenv.config({ path: process.env.DOTENV_CONFIG_PATH ?? "../../.env" });
import { GmxReadAdapter, type WalletAddress } from "./client.js";

const app = Fastify({ logger: true });
const adapter = new GmxReadAdapter();

const internalKey = process.env.GMX_ADAPTER_INTERNAL_KEY;
if (!internalKey) {
  throw new Error("GMX_ADAPTER_INTERNAL_KEY must be configured");
}

function jsonSafe(value: unknown): unknown {
  const serialized = JSON.stringify(value, (_key, item: unknown) =>
    typeof item === "bigint" ? item.toString() : item
  );
  return serialized === undefined ? null : JSON.parse(serialized);
}

app.addHook("onRequest", async (request, reply) => {
  const supplied = request.headers["x-internal-api-key"];
  if (typeof supplied !== "string" || supplied !== internalKey) {
    return reply.code(401).send({ error: "unauthorized" });
  }
});

app.get("/health", async () => ({
  status: "ok",
  service: "gmx-adapter",
  read_only: true
}));

app.get("/markets", async (_request, reply) => {
  try {
    const catalog = await adapter.getMarketCatalog();
    return reply.send(jsonSafe(catalog));
  } catch (error) {
    requestError(reply, error);
  }
});

app.get<{ Params: { address: string } }>("/account/:address", async (request, reply) => {
  const address = request.params.address;
  if (!/^0x[a-fA-F0-9]{40}$/.test(address)) {
    return reply.code(400).send({ error: "invalid_wallet_address" });
  }
  try {
    const snapshot = await adapter.getAccountSnapshot(address as WalletAddress);
    return reply.send(jsonSafe(snapshot));
  } catch (error) {
    requestError(reply, error);
  }
});

app.get<{ Querystring: { symbols?: string } }>("/tickers", async (request, reply) => {
  const symbols = request.query.symbols?.split(",").map((item) => item.trim()).filter(Boolean);
  if (!symbols?.length) {
    return reply.code(400).send({ error: "symbols_query_required" });
  }
  try {
    const tickers = await adapter.getMarketTickers(symbols);
    return reply.send(jsonSafe(tickers));
  } catch (error) {
    requestError(reply, error);
  }
});

function requestError(reply: FastifyReply, error: unknown) {
  app.log.error({ err: error }, "GMX read request failed");
  const message = error instanceof Error ? error.message : "GMX adapter request failed";
  return reply.code(502).send({ error: "gmx_read_failed", message });
}

const port = Number(process.env.GMX_ADAPTER_PORT ?? "8090");
const host = process.env.GMX_ADAPTER_HOST ?? "127.0.0.1";

await app.listen({ port, host });
