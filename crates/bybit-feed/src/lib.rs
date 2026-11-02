//! Public Bybit V5 market-data client. No API key is required for these endpoints.
//! The client fetches closed-history OHLCV candles only; callers must still avoid
//! treating the currently forming candle as a confirmed signal.

use chrono::{DateTime, Duration, Utc};
use futures_util::{SinkExt, StreamExt};
use fmt_strategy::Candle;
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::time::{interval_at, Duration as TokioDuration, Instant};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use thiserror::Error;

const PRODUCTION_BASE: &str = "https://api.bybit.com";
const TESTNET_BASE: &str = "https://api-testnet.bybit.com";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Production,
    Testnet,
}

impl Environment {
    fn websocket_url(self) -> &'static str {
        match self {
            Self::Production => "wss://stream.bybit.com/v5/public/linear",
            Self::Testnet => "wss://stream-testnet.bybit.com/v5/public/linear",
        }
    }

    fn base_url(self) -> &'static str {
        match self {
            Self::Production => PRODUCTION_BASE,
            Self::Testnet => TESTNET_BASE,
        }
    }
}

#[derive(Debug, Error)]
pub enum BybitError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid Bybit response: {0}")]
    InvalidResponse(String),
    #[error("Bybit API error {code}: {message}")]
    Api { code: i64, message: String },
    #[error("invalid candle timestamp: {0}")]
    InvalidTimestamp(i64),
    #[error("invalid candle numeric field: {0}")]
    InvalidNumber(String),
    #[error("Bybit WebSocket error: {0}")]
    WebSocket(String),
    #[error("Bybit WebSocket disconnected")]
    StreamClosed,
    #[error("unexpected instrument: expected active BTCUSDT LinearPerpetual")]
    InstrumentNotTradable,
}

#[derive(Debug, Clone)]
pub struct BybitClient {
    http: Client,
    base_url: Url,
}

impl BybitClient {
    pub fn new(environment: Environment) -> Result<Self, BybitError> {
        let base_url = Url::parse(environment.base_url())
            .map_err(|e| BybitError::InvalidResponse(e.to_string()))?;
        Ok(Self { http: Client::builder().build()?, base_url })
    }

    /// Confirm that BTCUSDT is currently a tradable linear perpetual before
    /// relying on it as the strategy's reference market.
    pub async fn ensure_btcusdt_linear_perpetual(&self) -> Result<(), BybitError> {
        let response: ApiEnvelope<InstrumentResult> = self
            .http
            .get(self.endpoint("v5/market/instruments-info")?)
            .query(&[("category", "linear"), ("symbol", "BTCUSDT")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        check_api(response.ret_code, response.ret_msg)?;
        let instrument = response.result.list.into_iter().find(|item| item.symbol == "BTCUSDT");
        match instrument {
            Some(item) if item.status == "Trading" && item.contract_type == "LinearPerpetual" => Ok(()),
            _ => Err(BybitError::InstrumentNotTradable),
        }
    }

    /// Fetch M15 BTCUSDT linear perpetual candles in [start, end].
    /// Bybit returns newest-first; this method sorts oldest-first for the engine.
    pub async fn fetch_btcusdt_m15(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: u16,
    ) -> Result<Vec<Candle>, BybitError> {
        if start > end || !(1..=1000).contains(&limit) {
            return Err(BybitError::InvalidResponse("invalid time range or limit".into()));
        }
        let start_ms = start.timestamp_millis().to_string();
        let end_ms = end.timestamp_millis().to_string();
        let limit_s = limit.to_string();
        let response: ApiEnvelope<KlineResult> = self
            .http
            .get(self.endpoint("v5/market/kline")?)
            .query(&[
                ("category", "linear"),
                ("symbol", "BTCUSDT"),
                ("interval", "15"),
                ("start", start_ms.as_str()),
                ("end", end_ms.as_str()),
                ("limit", limit_s.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        check_api(response.ret_code, response.ret_msg)?;

        let mut candles = response
            .result
            .list
            .into_iter()
            .map(parse_kline)
            .collect::<Result<Vec<_>, _>>()?;
        candles.sort_by_key(|c| c.open_time);
        candles.dedup_by_key(|c| c.open_time);
        // The REST endpoint can return the currently forming candle. FMT is
        // close-confirmed, so never pass a bar whose 15-minute interval is open.
        let now = Utc::now();
        candles.retain(|c| c.open_time + Duration::minutes(15) <= now);
        for candle in &candles {
            candle.validate().map_err(|e| BybitError::InvalidResponse(e.to_string()))?;
        }
        for pair in candles.windows(2) {
            let gap = pair[1].open_time.signed_duration_since(pair[0].open_time).num_minutes();
            if gap != 15 {
                return Err(BybitError::InvalidResponse(
                    "REST history contains a missing or misaligned M15 candle".into(),
                ));
            }
        }
        Ok(candles)
    }

    fn endpoint(&self, path: &str) -> Result<Url, BybitError> {
        self.base_url.join(path).map_err(|e| BybitError::InvalidResponse(e.to_string()))
    }
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    #[serde(rename = "retCode")]
    ret_code: i64,
    #[serde(rename = "retMsg")]
    ret_msg: String,
    result: T,
}

#[derive(Debug, Deserialize)]
struct KlineResult {
    list: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct InstrumentResult {
    list: Vec<Instrument>,
}

#[derive(Debug, Deserialize)]
struct Instrument {
    symbol: String,
    status: String,
    #[serde(rename = "contractType")]
    contract_type: String,
}

fn check_api(code: i64, message: String) -> Result<(), BybitError> {
    if code == 0 { Ok(()) } else { Err(BybitError::Api { code, message }) }
}

fn parse_kline(row: Vec<String>) -> Result<Candle, BybitError> {
    if row.len() < 6 {
        return Err(BybitError::InvalidResponse("kline row has fewer than six fields".into()));
    }
    let timestamp = row[0].parse::<i64>()
        .map_err(|_| BybitError::InvalidNumber("startTime".into()))?;
    let open_time = DateTime::from_timestamp_millis(timestamp)
        .ok_or(BybitError::InvalidTimestamp(timestamp))?;
    let number = |index: usize, field: &str| -> Result<f64, BybitError> {
        row[index].parse::<f64>().map_err(|_| BybitError::InvalidNumber(field.into()))
    };
    Ok(Candle {
        open_time,
        open: number(1, "openPrice")?,
        high: number(2, "highPrice")?,
        low: number(3, "lowPrice")?,
        close: number(4, "closePrice")?,
        // Bybit documents USDT/USDC contract volume in base-coin units.
        volume: number(5, "volume")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bybit_linear_kline_row() {
        let row = vec![
            "1768262400000".into(),
            "95000.0".into(),
            "95100.0".into(),
            "94900.0".into(),
            "95050.0".into(),
            "12.345".into(),
            "1173000.0".into(),
        ];
        let candle = parse_kline(row).unwrap();
        assert_eq!(candle.open, 95000.0);
        assert_eq!(candle.volume, 12.345);
        assert_eq!(candle.open_time.timestamp_millis(), 1768262400000);
    }

    #[test]
    fn rejects_short_kline_rows() {
        assert!(matches!(
            parse_kline(vec!["1".into(), "2".into()]),
            Err(BybitError::InvalidResponse(_))
        ));
    }

    #[test]
    fn parses_confirmed_websocket_kline_payload() {
        let row = serde_json::json!({
            "start": 1768262400000_i64,
            "interval": "15",
            "open": "95000.0",
            "high": "95100.0",
            "low": "94900.0",
            "close": "95050.0",
            "volume": "12.345",
            "confirm": true
        });
        let candle = parse_ws_kline(&row).unwrap();
        assert_eq!(candle.close, 95050.0);
        assert_eq!(candle.volume, 12.345);
    }
}


/// Public M15 kline stream. Only candles with Bybit's `confirm=true` flag
/// are passed to the strategy. On disconnect, the caller must reconnect and
/// backfill through REST before resuming signal evaluation.
pub struct BybitKlineStream {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    last_open_time: Option<DateTime<Utc>>,
}

impl BybitKlineStream {
    pub async fn connect(environment: Environment) -> Result<Self, BybitError> {
        Self::connect_after(environment, None).await
    }

    /// Connect after REST backfill, seeding the last processed candle timestamp.
    pub async fn connect_after(
        environment: Environment,
        last_open_time: Option<DateTime<Utc>>,
    ) -> Result<Self, BybitError> {
        let (mut socket, _) = connect_async(environment.websocket_url())
            .await
            .map_err(|e| BybitError::WebSocket(e.to_string()))?;
        let subscription = serde_json::json!({
            "op": "subscribe",
            "args": ["kline.15.BTCUSDT"]
        });
        socket
            .send(Message::Text(subscription.to_string().into()))
            .await
            .map_err(|e| BybitError::WebSocket(e.to_string()))?;
        Ok(Self { socket, last_open_time })
    }

    /// Runs until the connection closes or an invalid/out-of-order confirmed
    /// candle is received. The owner should reconnect with backoff and REST
    /// backfill; this method never silently resumes after a gap.
    pub async fn run_until_disconnect<F>(self, mut on_closed_candle: F) -> Result<(), BybitError>
    where
        F: FnMut(Candle) -> Result<(), String> + Send,
    {
        let (mut writer, mut reader) = self.socket.split();
        let mut last_open_time = self.last_open_time;
        let mut heartbeat = interval_at(
            Instant::now() + TokioDuration::from_secs(20),
            TokioDuration::from_secs(20),
        );

        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    let ping = serde_json::json!({"op": "ping"}).to_string();
                    writer.send(Message::Text(ping.into())).await
                        .map_err(|e| BybitError::WebSocket(e.to_string()))?;
                }
                incoming = reader.next() => {
                    let message = incoming.ok_or(BybitError::StreamClosed)?
                        .map_err(|e| BybitError::WebSocket(e.to_string()))?;
                    match message {
                        Message::Text(text) => {
                            let payload: Value = serde_json::from_str(text.as_ref())
                                .map_err(|e| BybitError::WebSocket(e.to_string()))?;
                            let Some(rows) = payload.get("data").and_then(Value::as_array) else {
                                continue;
                            };
                            for row in rows {
                                if row.get("confirm").and_then(Value::as_bool) != Some(true) {
                                    continue;
                                }
                                if row.get("interval").and_then(Value::as_str) != Some("15") {
                                    continue;
                                }
                                let candle = parse_ws_kline(row)?;
                                if let Some(last) = last_open_time.as_ref() {
                                    let gap_minutes = candle.open_time
                                        .signed_duration_since(*last)
                                        .num_minutes();
                                    if gap_minutes < 0 {
                                        return Err(BybitError::InvalidResponse(
                                            "out-of-order WebSocket candle; REST backfill required".into()
                                        ));
                                    }
                                    if gap_minutes == 0 {
                                        continue;
                                    }
                                    if gap_minutes != 15 {
                                        return Err(BybitError::InvalidResponse(
                                            "missing M15 candle; REST backfill required".into()
                                        ));
                                    }
                                }
                                candle.validate()
                                    .map_err(|e| BybitError::InvalidResponse(e.to_string()))?;
                                last_open_time = Some(candle.open_time);
                                on_closed_candle(candle).map_err(BybitError::InvalidResponse)?;
                            }
                        }
                        Message::Ping(payload) => {
                            writer.send(Message::Pong(payload)).await
                                .map_err(|e| BybitError::WebSocket(e.to_string()))?;
                        }
                        Message::Close(_) => return Err(BybitError::StreamClosed),
                        _ => {}
                    }
                }
            }
        }
    }
}

fn parse_ws_kline(row: &Value) -> Result<Candle, BybitError> {
    let timestamp = row.get("start").and_then(Value::as_i64)
        .ok_or_else(|| BybitError::InvalidNumber("start".into()))?;
    let open_time = DateTime::from_timestamp_millis(timestamp)
        .ok_or(BybitError::InvalidTimestamp(timestamp))?;
    let number = |field: &str| -> Result<f64, BybitError> {
        row.get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| BybitError::InvalidNumber(field.into()))?
            .parse::<f64>()
            .map_err(|_| BybitError::InvalidNumber(field.into()))
    };
    Ok(Candle {
        open_time,
        open: number("open")?,
        high: number("high")?,
        low: number("low")?,
        close: number("close")?,
        volume: number("volume")?,
    })
}
