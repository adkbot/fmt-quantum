use bybit_feed::{BybitClient, BybitKlineStream, Environment};
use chrono::{DateTime, Duration, Utc};
use fmt_strategy::{ExitReason, FmtEngine, Side, StrategyEvent};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, time::Duration as StdDuration};
use tokio::{sync::mpsc, time::sleep};
use tracing::{error, info, warn};

const KLINE_LIMIT: u16 = 1000;
const INITIAL_LOOKBACK_HOURS: i64 = 48;

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
}

impl SupabaseEventStore {
    fn from_env() -> Option<Self> {
        let base_url = env::var("SUPABASE_URL").ok()?;
        let service_role_key = env::var("SUPABASE_SERVICE_ROLE_KEY").ok()?;
        Some(Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            service_role_key,
        })
    }

    fn authenticated_request(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
            .header("apikey", &self.service_role_key)
            .bearer_auth(&self.service_role_key)
    }

    async fn persist_event(&self, event: StrategyEvent) -> Result<(), String> {
        let (event_type, event_time, reference_price, detail) = match event {
            StrategyEvent::Enter { side, price, time } => {
                let event_type = match side {
                    Side::Buy => "ENTER_BUY",
                    Side::Sell => "ENTER_SELL",
                };
                (
                    event_type,
                    time,
                    price,
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
                    json!({"side": format!("{side:?}"), "reason": format!("{reason:?}"), "source": "BYBIT", "symbol": "BTCUSDT", "timeframe": "M15"}),
                )
            }
        };

        let strategies_url = format!("{}/rest/v1/strategy_instances", self.base_url);
        let strategies = self
            .authenticated_request(self.http.get(strategies_url))
            .query(&[
                ("select", "id,user_id"),
                ("is_enabled", "eq.true"),
                ("strategy_code", "eq.FMT_TOKYO_M15"),
                ("limit", "1000"),
            ])
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json::<Vec<ActiveStrategy>>()
            .await
            .map_err(|error| error.to_string())?;

        if strategies.is_empty() {
            info!("No enabled FMT strategy instances; event not assigned to a user");
            return Ok(());
        }

        let events_url = format!("{}/rest/v1/strategy_events", self.base_url);
        for strategy in strategies {
            let row: Value = json!({
                "user_id": strategy.user_id,
                "strategy_instance_id": strategy.id,
                "event_type": event_type,
                "source_candle_open_time": event_time.to_rfc3339(),
                "reference_price": reference_price,
                "payload": detail,
            });
            self.authenticated_request(self.http.post(&events_url))
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
        }
        Ok(())
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

async fn persist_events(store: Option<&SupabaseEventStore>, events: Vec<StrategyEvent>) {
    let Some(store) = store else {
        if !events.is_empty() {
            warn!("Supabase event persistence is not configured; events exist only in logs");
        }
        return;
    };
    for event in events {
        if let Err(error) = store.persist_event(event).await {
            error!(%error, "Failed to persist FMT strategy event");
        }
    }
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
        let events = process_candles(engine, last_processed, candles)?;
        persist_events(store, events).await;
    }
    Ok(())
}

#[tokio::main]
async fn main() {
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

    let mut backoff_seconds = 1_u64;
    loop {
        let mut connected = false;
        match BybitKlineStream::connect_after(environment, last_processed).await {
            Ok(stream) => {
                connected = true;
                info!(last = ?last_processed, "Connected to Bybit confirmed M15 WebSocket");

                let (event_tx, mut event_rx) = mpsc::unbounded_channel::<StrategyEvent>();
                let store_for_task = store.clone();
                let persistence_task = tokio::spawn(async move {
                    while let Some(event) = event_rx.recv().await {
                        persist_events(store_for_task.as_ref(), vec![event]).await;
                    }
                });
                let callback_tx = event_tx.clone();
                let mut callback_error: Option<String> = None;

                let result = stream
                    .run_until_disconnect(|candle| {
                        if callback_error.is_some() {
                            return;
                        }
                        let candle_time = candle.open_time;
                        match engine.on_candle(candle) {
                            Ok(events) => {
                                last_processed = Some(candle_time);
                                log_events(&events);
                                for event in events {
                                    if callback_tx.send(event).is_err() {
                                        callback_error = Some("event persistence queue closed".into());
                                        break;
                                    }
                                }
                            }
                            Err(error) => {
                                callback_error = Some(error.to_string());
                            }
                        }
                    })
                    .await;

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
