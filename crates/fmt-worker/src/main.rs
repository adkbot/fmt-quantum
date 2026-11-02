use bybit_feed::{BybitClient, BybitKlineStream, Environment};
use chrono::{DateTime, Duration, Utc};
use fmt_strategy::{FmtEngine, StrategyEvent};
use std::{env, time::Duration as StdDuration};
use tokio::time::sleep;
use tracing::{error, info, warn};

const KLINE_LIMIT: u16 = 1000;
const INITIAL_LOOKBACK_HOURS: i64 = 48;

fn log_events(events: Vec<StrategyEvent>) {
    for event in events {
        info!(event = ?event, "FMT strategy event");
    }
}

fn process_candles(
    engine: &mut FmtEngine,
    last_processed: &mut Option<DateTime<Utc>>,
    candles: Vec<fmt_strategy::Candle>,
) -> Result<(), String> {
    for candle in candles {
        let candle_time = candle.open_time;
        let events = engine.on_candle(candle).map_err(|error| error.to_string())?;
        *last_processed = Some(candle_time);
        log_events(events);
    }
    Ok(())
}

async fn backfill(
    client: &BybitClient,
    engine: &mut FmtEngine,
    last_processed: &mut Option<DateTime<Utc>>,
    initial: bool,
) -> Result<(), String> {
    let end = Utc::now();
    let start = last_processed
        .map(|last| last + Duration::minutes(15))
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
        process_candles(engine, last_processed, candles)?;
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

    let client = BybitClient::new(environment).expect("failed to create Bybit client");
    client
        .ensure_btcusdt_linear_perpetual()
        .await
        .expect("BTCUSDT linear perpetual is not confirmed as Trading");

    let mut engine = FmtEngine::new();
    let mut last_processed: Option<DateTime<Utc>> = None;

    backfill(&client, &mut engine, &mut last_processed, true)
        .await
        .expect("initial FMT history warm-up failed");

    let mut backoff_seconds = 1_u64;
    loop {
        match BybitKlineStream::connect_after(environment, last_processed).await {
            Ok(stream) => {
                info!(last = ?last_processed, "Connected to Bybit confirmed M15 WebSocket");
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
                                log_events(events);
                            }
                            Err(error) => {
                                callback_error = Some(error.to_string());
                            }
                        }
                    })
                    .await;

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

        // Reconstruct any missed closed bars before reconnecting. The strategy
        // engine rejects gaps, so it cannot silently trade on an incomplete series.
        if let Err(error) = backfill(&client, &mut engine, &mut last_processed, false).await {
            error!(%error, "Bybit REST backfill failed; keeping worker fail-closed");
        } else {
            backoff_seconds = 1;
        }

        sleep(StdDuration::from_secs(backoff_seconds)).await;
        backoff_seconds = (backoff_seconds * 2).min(30);
    }
}
