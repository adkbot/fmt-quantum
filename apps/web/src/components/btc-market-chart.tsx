"use client";

import { useEffect, useRef, useState } from "react";
import {
  CandlestickSeries,
  createChart,
  type CandlestickData,
  type Time,
  type UTCTimestamp
} from "lightweight-charts";
import { supabase } from "@/lib/supabase";

type CandleRow = {
  id: number;
  open_time: string;
  open: number | string;
  high: number | string;
  low: number | string;
  close: number | string;
};

function toChartCandle(row: CandleRow): CandlestickData<UTCTimestamp> {
  return {
    time: Math.floor(Date.parse(row.open_time) / 1000) as UTCTimestamp,
    open: Number(row.open),
    high: Number(row.high),
    low: Number(row.low),
    close: Number(row.close)
  };
}

function newYorkTime(time: Time) {
  return new Date(Number(time) * 1000).toLocaleString("en-US", {
    timeZone: "America/New_York",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  });
}

export function BtcMarketChart() {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const latestTimeRef = useRef<number | null>(null);
  const [userId, setUserId] = useState<string | null>(null);
  const [message, setMessage] = useState("Entre na conta para carregar o histórico M15.");

  useEffect(() => {
    if (!supabase) return;
    void supabase.auth.getSession().then(({ data }) => {
      setUserId(data.session?.user.id ?? null);
    });
    const { data } = supabase.auth.onAuthStateChange((_event, session) => {
      setUserId(session?.user.id ?? null);
    });
    return () => data.subscription.unsubscribe();
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!supabase || !userId || !container) {
      setMessage("Entre na conta para carregar o histórico M15.");
      return;
    }

    const chart = createChart(container, {
      width: container.clientWidth,
      height: 340,
      layout: {
        background: { color: "#0B0F14" },
        textColor: "#A9B8C6"
      },
      grid: {
        vertLines: { color: "#18232D" },
        horzLines: { color: "#18232D" }
      },
      rightPriceScale: { borderColor: "#263541" },
      timeScale: {
        borderColor: "#263541",
        timeVisible: true,
        secondsVisible: false,
        tickMarkFormatter: (time) => newYorkTime(time)
      },
      localization: {
        locale: "en-US",
        timeFormatter: (time) => newYorkTime(time)
      },
      crosshair: { mode: 1 }
    });

    const series = chart.addSeries(CandlestickSeries, {
      upColor: "#26A69A",
      downColor: "#EF5350",
      borderVisible: false,
      wickUpColor: "#26A69A",
      wickDownColor: "#EF5350"
    });

    let cancelled = false;
    let loaded = false;
    const pending: CandleRow[] = [];

    const channel = supabase
      .channel("bybit-btcusdt-m15-market-candles")
      .on(
        "postgres_changes",
        {
          event: "*",
          schema: "public",
          table: "market_candles",
          filter: "source_symbol=eq.BTCUSDT"
        },
        (payload) => {
          if (payload.eventType === "DELETE") return;
          const row = payload.new as CandleRow;
          if (!row.open_time) return;
          if (!loaded) {
            pending.push(row);
            return;
          }
          const candle = toChartCandle(row);
          const candleTime = Number(candle.time);
          if (latestTimeRef.current === null || candleTime >= latestTimeRef.current) {
            series.update(candle);
            latestTimeRef.current = candleTime;
            setMessage("Candles Bybit M15 atualizados em tempo real · horário de Nova York.");
          }
        }
      )
      .subscribe((status) => {
        if (status === "CHANNEL_ERROR" || status === "TIMED_OUT") {
          setMessage("Canal Realtime indisponível; confira a publicação e as políticas RLS.");
        }
      });

    const observer = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width;
      if (width) chart.applyOptions({ width });
    });
    observer.observe(container);

    async function loadHistory() {
      const { data, error } = await supabase
        .from("market_candles")
        .select("id,open_time,open,high,low,close")
        .eq("source_venue", "BYBIT")
        .eq("source_category", "linear")
        .eq("source_symbol", "BTCUSDT")
        .eq("timeframe_minutes", 15)
        .order("open_time", { ascending: false })
        .limit(500);

      if (cancelled) return;
      if (error) {
        setMessage("Não foi possível carregar candles. Verifique a migration, RLS e o worker.");
        return;
      }

      const rows = new Map<number, CandleRow>();
      for (const row of [...(data ?? []), ...pending]) {
        rows.set(Math.floor(Date.parse(row.open_time) / 1000), row as CandleRow);
      }
      const ordered = [...rows.values()].sort(
        (a, b) => Date.parse(a.open_time) - Date.parse(b.open_time)
      );
      const candles = ordered.map(toChartCandle);
      series.setData(candles);
      latestTimeRef.current = candles.length ? Number(candles[candles.length - 1].time) : null;
      chart.timeScale().fitContent();
      loaded = true;
      setMessage(candles.length
        ? "Histórico Bybit M15 carregado · aguardando próximos candles confirmados."
        : "Nenhum candle salvo ainda. Inicie o worker Bybit para alimentar o gráfico.");

      for (const row of pending) {
        const candle = toChartCandle(row);
        const candleTime = Number(candle.time);
        if (latestTimeRef.current === null || candleTime >= latestTimeRef.current) {
          series.update(candle);
          latestTimeRef.current = candleTime;
        }
      }
      pending.length = 0;
    }

    void loadHistory();

    return () => {
      cancelled = true;
      void supabase.removeChannel(channel);
      observer.disconnect();
      chart.remove();
    };
  }, [userId]);

  return (
    <section className="market-chart panel">
      <div className="panel-heading">
        <div>
          <div className="eyebrow">BYBIT · LINEAR PERPETUAL</div>
          <h2>BTCUSDT · 15 minutos</h2>
        </div>
        <span className="tag">NEW YORK TIME</span>
      </div>
      <div ref={containerRef} className="chart-container" />
      <p className="api-status">{message}</p>
      <p className="muted-copy">Fonte OHLCV usada pelo motor FMT. O gráfico mostra candles confirmados; não representa preço de execução GMX.</p>
    </section>
  );
}
