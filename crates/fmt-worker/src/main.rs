use bybit_feed::{BybitClient, BybitKlineStream, Environment};
use chrono::{DateTime, Duration, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::America::New_York;
use fmt_strategy::{Candle, FmtEngine, Side, StrategyEvent};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, time::Duration as StdDuration};
use tokio::{sync::mpsc, time::sleep};
use tracing::{error, info, warn};

const KLINE_LIMIT: u16 = 1000;
const INITIAL_LOOKBACK_HOURS: i64 = 48;
const DEFAULT_HISTORY_DAYS: i64 = 60;
const MAX_HISTORY_DAYS: i64 = 400;

#[derive(Clone)]
struct SupabaseEventStore {
    http: Client,
    base_url: String,
    service_role_key: String,
}

#[derive(Debug, Deserialize)]
struct ActiveStrategy {
    id: String,
    user_id: String,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
enum EventOrigin {
    RestBackfill,
    LiveWebSocket,
}

impl EventOrigin {
    fn as_str(self) -> &'static str {
        match self {
            Self::RestBackfill => "REST_BACKFILL",
            Self::LiveWebSocket => "LIVE_WEBSOCKET",
        }
    }

    fn is_live_signal(self) -> bool {
        matches!(self, Self::LiveWebSocket)
    }
}

enum WorkerMessage {
    CandleAndEvents { candle: Candle, events: Vec<StrategyEvent> },
}

fn gross_pnl_bps(side: &str, entry_price: f64, exit_price: f64) -> Result<i64, String> {
    if !entry_price.is_finite() || !exit_price.is_finite() || entry_price <= 0.0 || exit_price <= 0.0 {
        return Err("paper prices must be finite and greater than zero".into());
    }
    let value = match side {
        "BUY" => ((exit_price - entry_price) / entry_price) * 10_000.0,
        "SELL" => ((entry_price - exit_price) / entry_price) * 10_000.0,
        _ => return Err("paper position side is invalid".into()),
    };
    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        return Err("paper PnL is outside the supported range".into());
    }
    Ok(value.round() as i64)
}

fn session_start_for_event(event_time: DateTime<Utc>) -> DateTime<Utc> {
    let local = event_time.with_timezone(&New_York);
    let date = local.date_naive();
    let session_date = if local.hour() < 6 {
        date.pred_opt().expect("valid previous New York calendar date")
    } else {
        date
    };
    let local_start = NaiveDateTime::new(
        session_date,
        NaiveTime::from_hms_opt(20, 0, 0).expect("valid session start time"),
    );
    New_York
        .from_local_datetime(&local_start)
        .single()
        .expect("20:00 New York session start is not ambiguous")
        .with_timezone(&Utc)
}

impl SupabaseEventStore {
    fn from_env() -> Option<Self> {
        let base_url = env::var("SUPABASE_URL").ok()?;
        let service_role_key = env::var("SUPABASE_SERVICE_ROLE_KEY").ok()?;
        Some(Self {
            http: Client::builder()
                .timeout(StdDuration::from_secs(15))
                .build()
                .expect("failed to create Supabase HTTP client"),
            base_url: base_url.trim_end_matches('/').to_string(),
            service_role_key,
        })
    }

    fn authenticated_request(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
            .header("apikey", &self.service_role_key)
            .bearer_auth(&self.service_role_key)
    }

    async fn persist_candles(&self, candles: &[Candle]) -> Result<(), String> {
        if candles.is_empty() {
            return Ok(());
        }
        let rows: Vec<Value> = candles
            .iter()
            .map(|candle| {
                json!({
                    "source_venue": "BYBIT",
                    "source_category": "linear",
                    "source_symbol": "BTCUSDT",
                    "timeframe_minutes": 15,
                    "open_time": candle.open_time.to_rfc3339(),
                    "open": candle.open,
                    "high": candle.high,
                    "low": candle.low,
                    "close": candle.close,
                    "volume": candle.volume
                })
            })
            .collect();
        let url = format!("{}/rest/v1/market_candles", self.base_url);
        self.authenticated_request(self.http.post(url.as_str()))
            .query(&[(
                "on_conflict",
                "source_venue,source_category,source_symbol,timeframe_minutes,open_time",
            )])
            .header("Prefer", "resolution=merge-duplicates,return=minimal")
            .json(&rows)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn apply_paper_event(
        &self,
        strategy_id: &str,
        user_id: &str,
        side: &str,
        is_entry: bool,
        price: f64,
        event_time: DateTime<Utc>,
        origin: EventOrigin,
    ) -> Result<(), String> {
        if !price.is_finite() || price <= 0.0 {
            return Err("paper event has an invalid reference price".into());
        }

        let positions_url = format!("{}/rest/v1/paper_positions", self.base_url);
        let open_positions = self
            .authenticated_request(self.http.get(positions_url.as_str()))
            .query(&[
                ("select", "id,side,entry_price"),
                ("strategy_instance_id", &format!("eq.{strategy_id}")),
                ("status", "eq.OPEN"),
                ("limit", "1"),
            ])
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json::<Vec<Value>>()
            .await
            .map_err(|error| error.to_string())?;

        if is_entry {
            if !open_positions.is_empty() {
                return Ok(());
            }

            let row = json!({
                "user_id": user_id,
                "strategy_instance_id": strategy_id,
                "side": side,
                "entry_price": price,
                "entry_candle_open_time": event_time.to_rfc3339(),
                "entry_origin": origin.as_str(),
                "status": "OPEN"
            });
            self.authenticated_request(self.http.post(positions_url.as_str()))
                .query(&[("on_conflict", "strategy_instance_id,entry_candle_open_time")])
                .header("Prefer", "resolution=ignore-duplicates,return=minimal")
                .json(&row)
                .send()
                .await
                .map_err(|error| error.to_string())?
                .error_for_status()
                .map_err(|error| error.to_string())?;
            return Ok(());
        }

        let Some(open) = open_positions.first() else {
            return Ok(());
        };
        let open_side = open
            .get("side")
            .and_then(Value::as_str)
            .ok_or_else(|| "paper position side is missing".to_string())?;
        if open_side != side {
            warn!(
                strategy_id,
                open_side,
                exit_side = side,
                "Ignoring paper exit whose side does not match the open paper position"
            );
            return Ok(());
        }

        let id = open
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| "paper position id is missing".to_string())?;
        let entry_price = open
            .get("entry_price")
            .and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))
            .ok_or_else(|| "paper position entry price is invalid".to_string())?;
        let gross_pnl_bps = gross_pnl_bps(side, entry_price, price)?;

        let id_filter = format!("eq.{id}");
        let open_filter = "eq.OPEN";
        let update = json!({
            "status": "CLOSED",
            "exit_price": price,
            "exit_candle_open_time": event_time.to_rfc3339(),
            "pnl_bps": gross_pnl_bps
        });
        self.authenticated_request(self.http.patch(positions_url.as_str()))
            .query(&[("id", id_filter.as_str()), ("status", open_filter)])
            .header("Prefer", "return=minimal")
            .json(&update)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn persist_event(&self, event: StrategyEvent, origin: EventOrigin) -> Result<(), String> {
        let (event_type, event_time, reference_price, side, is_entry, mut detail) = match event {
            StrategyEvent::Enter { side, price, time } => {
                let event_type = match side {
                    Side::Buy => "ENTER_BUY",
                    Side::Sell => "ENTER_SELL",
                };
                (
                    event_type,
                    time,
                    price,
                    if side == Side::Buy { "BUY" } else { "SELL" },
                    true,
                    json!({"side": format!("{side:?}"), "source": "BYBIT", "symbol": "BTCUSDT", "timeframe": "M15"}),
                )
            }
            StrategyEvent::Exit { side, price, time, reason } => {
                let event_type = match side {
                    Side::Buy => "EXIT_BUY",
                    Side::Sell => "EXIT_SELL",
                };
                (
                    event_type,
                    time,
                    price,
                    if side == Side::Buy { "BUY" } else { "SELL" },
                    false,
                    json!({"side": format!("{side:?}"), "reason": format!("{reason:?}"), "source": "BYBIT", "symbol": "BTCUSDT", "timeframe": "M15"}),
                )
            }
        };
        if let Some(payload) = detail.as_object_mut() {
            payload.insert("origin".into(), json!(origin.as_str()));
            payload.insert("is_live_signal".into(), json!(origin.is_live_signal()));
            payload.insert("execution_eligible".into(), json!(false));
        }

        let strategies_url = format!("{}/rest/v1/strategy_instances", self.base_url);
        let mut strategies = Vec::new();
        let mut offset = 0_usize;
        loop {
            let range = format!("{}-{}", offset, offset + 999);
            let page = self
                .authenticated_request(self.http.get(strategies_url.as_str()))
                .query(&[
                    ("select", "id,user_id,updated_at"),
                    ("is_enabled", "eq.true"),
                    ("strategy_code", "eq.FMT_TOKYO_M15"),
                    ("source_symbol", "eq.BTCUSDT"),
                    ("order", "id.asc"),
                    ("limit", "1000"),
                ])
                .header("Range", range)
                .send()
                .await
                .map_err(|error| error.to_string())?
                .error_for_status()
                .map_err(|error| error.to_string())?
                .json::<Vec<ActiveStrategy>>()
                .await
                .map_err(|error| error.to_string())?;
            let page_len = page.len();
            strategies.extend(page);
            if page_len < 1000 {
                break;
            }
            offset += 1000;
        }

        if strategies.is_empty() {
            info!("No enabled FMT strategy instances; event not assigned to a user");
            return Ok(());
        }

        let events_url = format!("{}/rest/v1/strategy_events", self.base_url);
        let session_start = session_start_for_event(event_time);
        for strategy in strategies {
            // A strategy enabled or re-enabled after the current session began
            // joins on the next session, preventing orphan exit signals.
            if strategy.updated_at > session_start {
                continue;
            }
            let row: Value = json!({
                "user_id": strategy.user_id,
                "strategy_instance_id": strategy.id,
                "event_type": event_type,
                "source_candle_open_time": event_time.to_rfc3339(),
                "reference_price": reference_price,
                "payload": detail.clone(),
            });
            self.authenticated_request(self.http.post(events_url.as_str()))
                .query(&[(
                    "on_conflict",
                    "user_id,strategy_instance_id,source_candle_open_time,event_type",
                )])
                .header("Prefer", "resolution=ignore-duplicates,return=minimal")
                .json(&row)
                .send()
                .await
                .map_err(|error| error.to_string())?
                .error_for_status()
                .map_err(|error| error.to_string())?;

            self.apply_paper_event(
                &strategy.id,
                &strategy.user_id,
                side,
                is_entry,
                reference_price,
                event_time,
                origin,
            )
            .await?;
        }
        Ok(())
    }
}

async fn persist_candles_with_retry(store: &SupabaseEventStore, candles: &[Candle]) {
    let mut backoff = 1_u64;
    loop {
        match store.persist_candles(candles).await {
            Ok(()) => return,
            Err(error) => {
                error!(%error, backoff_seconds = backoff, "Candle persistence failed; retrying");
                sleep(StdDuration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(60);
            }
        }
    }
}

async fn persist_event_with_retry(store: &SupabaseEventStore, event: StrategyEvent, origin: EventOrigin) {
    let mut backoff = 1_u64;
    loop {
        match store.persist_event(event.clone(), origin).await {
            Ok(()) => return,
            Err(error) => {
                error!(%error, backoff_seconds = backoff, "FMT event persistence failed; retrying");
                sleep(StdDuration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(60);
            }
        }
    }
}

fn log_events(events: &[StrategyEvent]) {
    for event in events {
        info!(event = ?event, "FMT strategy event");
    }
}

fn process_candles(
    engine: &mut FmtEngine,
    last_processed: &mut Option<DateTime<Utc>>,
    candles: Vec<fmt_strategy::Candle>,
) -> Result<Vec<StrategyEvent>, String> {
    let mut emitted = Vec::new();
    for candle in candles {
        let candle_time = candle.open_time;
        let events = engine.on_candle(candle).map_err(|error| error.to_string())?;
        *last_processed = Some(candle_time);
        log_events(&events);
        emitted.extend(events);
    }
    Ok(emitted)
}

async fn backfill(
    client: &BybitClient,
    engine: &mut FmtEngine,
    last_processed: &mut Option<DateTime<Utc>>,
    store: Option<&SupabaseEventStore>,
    initial: bool,
) -> Result<(), String> {
    let end = Utc::now();
    let start = last_processed
        .as_ref()
        .map(|last| *last + Duration::minutes(15))
        .unwrap_or_else(|| end - Duration::hours(INITIAL_LOOKBACK_HOURS));

    if start > end {
        return Ok(());
    }

    let candles = client
        .fetch_btcusdt_m15(start, end, KLINE_LIMIT)
        .await
        .map_err(|error| error.to_string())?;

    if initial && candles.is_empty() {
        return Err("Bybit returned no closed M15 candles for initial warm-up".into());
    }

    if !candles.is_empty() {
        info!(count = candles.len(), "Processing Bybit REST backfill");
        let candles_for_storage = candles.clone();
        let events = process_candles(engine, last_processed, candles)?;
        if let Some(store) = store {
            persist_candles_with_retry(store, &candles_for_storage).await;
        }
        if let Some(store) = store {
            for event in events {
                persist_event_with_retry(store, event, EventOrigin::RestBackfill).await;
            }
        } else if !events.is_empty() {
            info!(
                count = events.len(),
                "Historical FMT events suppressed because Supabase persistence is not configured"
            );
        }
    }
    Ok(())
}

/// Loads closed M15 candles for the last `days` days into `market_candles`.
/// Storage only: it never feeds the strategy engine or emits events, so it
/// cannot create signals or paper positions. Failures are logged and ignored.
async fn load_history(client: &BybitClient, store: &SupabaseEventStore, days: i64) {
    if days <= 0 {
        return;
    }
    let days = days.min(MAX_HISTORY_DAYS);
    let step = Duration::minutes(15);
    let now = Utc::now();
    let floor_ts = now.timestamp() - now.timestamp().rem_euclid(15 * 60);
    let latest_open = Utc
        .timestamp_opt(floor_ts, 0)
        .single()
        .map(|t| t - step)
        .unwrap_or(now);
    let oldest_wanted = now - Duration::days(days);
    let mut end = latest_open;
    let mut total = 0_usize;
    while end > oldest_wanted {
        let start = std::cmp::max(end - step * (i32::from(KLINE_LIMIT) - 1), oldest_wanted);
        match client.fetch_btcusdt_m15(start, end, KLINE_LIMIT).await {
            Ok(candles) if candles.is_empty() => break,
            Ok(candles) => {
                if let Err(error) = store.persist_candles(&candles).await {
                    warn!(%error, "History candle persistence failed; stopping history load");
                    break;
                }
                total += candles.len();
            }
            Err(error) => {
                warn!(%error, "Bybit history page failed; stopping history load");
                break;
            }
        }
        end = start - step;
        sleep(StdDuration::from_millis(250)).await;
    }
    info!(candles = total, days, "Historical M15 candles stored for backtesting");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_gross_pnl_handles_long_and_short_positions() {
        assert_eq!(gross_pnl_bps("BUY", 100.0, 101.0).unwrap(), 100);
        assert_eq!(gross_pnl_bps("SELL", 100.0, 99.0).unwrap(), 100);
        assert_eq!(gross_pnl_bps("BUY", 100.0, 99.0).unwrap(), -100);
        assert_eq!(gross_pnl_bps("SELL", 100.0, 101.0).unwrap(), -100);
    }

    #[test]
    fn paper_gross_pnl_rejects_invalid_prices() {
        assert!(gross_pnl_bps("BUY", 0.0, 101.0).is_err());
        assert!(gross_pnl_bps("HOLD", 100.0, 101.0).is_err());
    }

    #[test]
    fn overnight_signal_uses_previous_new_york_session_start() {
        let event_time = New_York
            .with_ymd_and_hms(2026, 1, 13, 1, 15, 0)
            .unwrap()
            .with_timezone(&Utc);
        let session_start = session_start_for_event(event_time);
        let expected = New_York
            .with_ymd_and_hms(2026, 1, 12, 20, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(session_start, expected);
    }

    #[test]
    fn summer_signal_uses_dst_aware_new_york_session_start() {
        let event_time = New_York
            .with_ymd_and_hms(2026, 7, 13, 21, 15, 0)
            .unwrap()
            .with_timezone(&Utc);
        let session_start = session_start_for_event(event_time);
        let expected = New_York
            .with_ymd_and_hms(2026, 7, 13, 20, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(session_start, expected);
    }
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG").unwrap_or_else(|_| "fmt_worker=info".to_string()),
        )
        .init();

    let environment = match env::var("BYBIT_ENV").unwrap_or_else(|_| "production".into()).as_str() {
        "testnet" => Environment::Testnet,
        "production" => Environment::Production,
        other => panic!("BYBIT_ENV must be production or testnet, got {other}"),
    };

    let store = SupabaseEventStore::from_env();
    if store.is_none() {
        warn!("SUPABASE_URL or SUPABASE_SERVICE_ROLE_KEY missing; event persistence disabled");
    }

    let client = BybitClient::new(environment).expect("failed to create Bybit client");
    client
        .ensure_btcusdt_linear_perpetual()
        .await
        .expect("BTCUSDT linear perpetual is not confirmed as Trading");

    let mut engine = FmtEngine::new();
    let mut last_processed: Option<DateTime<Utc>> = None;

    backfill(&client, &mut engine, &mut last_processed, store.as_ref(), true)
        .await
        .expect("initial FMT history warm-up failed");

    if let Some(store) = store.as_ref() {
        let history_days = env::var("HISTORY_DAYS")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(DEFAULT_HISTORY_DAYS);
        load_history(&client, store, history_days).await;
    }

    let mut backoff_seconds = 1_u64;
    loop {
        let mut connected = false;
        match BybitKlineStream::connect_after(environment, last_processed).await {
            Ok(stream) => {
                connected = true;
                info!(last = ?last_processed, "Connected to Bybit confirmed M15 WebSocket");

                let (event_tx, mut event_rx) = mpsc::channel::<WorkerMessage>(256);
                let store_for_task = store.clone();
                let persistence_task = tokio::spawn(async move {
                    while let Some(event) = event_rx.recv().await {
                        match (store_for_task.as_ref(), event) {
                            (Some(store), WorkerMessage::CandleAndEvents { candle, events }) => {
                                persist_candles_with_retry(&store, &[candle]).await;
                                for event in events {
                                    persist_event_with_retry(&store, event, EventOrigin::LiveWebSocket).await;
                                }
                            }
                            (None, _) => {}
                        }
                    }
                });
                let callback_tx = event_tx.clone();
                let mut callback_error: Option<String> = None;

                let result = stream
                    .run_until_disconnect(|candle| {
                        if callback_error.is_some() {
                            return Err("strategy handler already failed".into());
                        }
                        let candle_time = candle.open_time;
                        let engine_before = engine.clone();
                        match engine.on_candle(candle) {
                            Ok(events) => {
                                log_events(&events);
                                if store.is_some()
                                    && callback_tx.try_send(WorkerMessage::CandleAndEvents {
                                        candle,
                                        events: events.clone(),
                                    }).is_err()
                                {
                                    engine = engine_before;
                                    let error = "market persistence queue is full or closed; candle will be replayed".to_string();
                                    callback_error = Some(error.clone());
                                    return Err(error);
                                }
                                last_processed = Some(candle_time);
                                Ok(())
                            }
                            Err(error) => {
                                let message = error.to_string();
                                callback_error = Some(message.clone());
                                Err(message)
                            }
                        }
                    })
                    .await;

                drop(callback_tx);
                drop(event_tx);
                if let Err(error) = persistence_task.await {
                    error!(%error, "Event persistence task failed");
                }

                if let Some(error) = callback_error {
                    error!(%error, "FMT engine rejected a WebSocket candle");
                } else if let Err(error) = result {
                    warn!(%error, "Bybit stream ended; backfill required before reconnect");
                }
            }
            Err(error) => {
                warn!(%error, "Unable to connect to Bybit WebSocket");
            }
        }

        if let Err(error) = backfill(
            &client,
            &mut engine,
            &mut last_processed,
            store.as_ref(),
            false,
        )
        .await
        {
            error!(%error, "Bybit REST backfill failed; keeping worker fail-closed");
        } else if connected {
            backoff_seconds = 1;
        }

        sleep(StdDuration::from_secs(backoff_seconds)).await;
        backoff_seconds = (backoff_seconds * 2).min(30);
    }
}
