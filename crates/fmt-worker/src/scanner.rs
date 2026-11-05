//! Multi-asset FMT scanner (paper mode only).
//!
//! Every closed M15 candle the scanner fetches the last ~50 hours for the top-N most liquid
//! Bybit USDT linear perpetuals, replays the FMT engine from scratch (stateless, so a restart
//! never loses state) and records opportunities found on the latest candles.
//!
//! Users with the scanner enabled (or an explicit instance for that symbol) receive the event;
//! paper entries are opened automatically ONLY when the user's own limits allow it:
//! auto-entry on, a saved risk configuration, and fewer open paper positions than
//! `max_open_positions`. No real order is ever created here.

use crate::executor::{self, ExecConfig, ExecRequest};
use bybit_feed::{BybitClient, Environment, TickerSummary};
use chrono::{DateTime, Duration, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::America::New_York;
use fmt_strategy::{Candle, FmtEngine, Side, StrategyEvent};
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    env,
    time::Duration as StdDuration,
};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

const WARMUP_HOURS: i64 = 50;
const SCAN_DELAY_SECONDS: i64 = 12;
const UNIVERSE_REFRESH_MINUTES: i64 = 60;
const DEFAULT_TOP_N: usize = 30;
const MAX_TOP_N: usize = 100;
const SESSION_LENGTH_HOURS: i64 = 10;
const CANDLE_RETENTION_DAYS: i64 = 8;

#[derive(Clone)]
pub(crate) struct Db {
    http: Client,
    base: String,
    key: String,
}

impl Db {
    pub(crate) fn from_env() -> Option<Self> {
        let base = env::var("SUPABASE_URL").ok()?;
        let key = env::var("SUPABASE_SERVICE_ROLE_KEY").ok()?;
        Some(Self {
            http: Client::builder().timeout(StdDuration::from_secs(20)).build().ok()?,
            base: base.trim_end_matches('/').to_string(),
            key,
        })
    }

    pub(crate) fn url(&self, table: &str) -> String {
        format!("{}/rest/v1/{}", self.base, table)
    }

    pub(crate) fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        rb.header("apikey", &self.key).bearer_auth(&self.key)
    }

    pub(crate) async fn get(&self, table: &str, query: &[(&str, String)]) -> Result<Vec<Value>, String> {
        let mut q: Vec<(&str, String)> = query.to_vec();
        q.push(("limit", "1000".into()));
        self.auth(self.http.get(self.url(table)))
            .query(&q)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json::<Vec<Value>>()
            .await
            .map_err(|e| e.to_string())
    }

    pub(crate) async fn post(&self, table: &str, query: &[(&str, String)], prefer: &str, body: &Value) -> Result<Value, String> {
        let response = self
            .auth(self.http.post(self.url(table)))
            .query(query)
            .header("Prefer", prefer)
            .json(body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        if prefer.contains("return=representation") {
            response.json::<Value>().await.map_err(|e| e.to_string())
        } else {
            Ok(Value::Null)
        }
    }

    pub(crate) async fn patch(&self, table: &str, query: &[(&str, String)], body: &Value) -> Result<(), String> {
        self.auth(self.http.patch(self.url(table)))
            .query(query)
            .header("Prefer", "return=minimal")
            .json(body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub(crate) async fn delete(&self, table: &str, query: &[(&str, String)]) -> Result<(), String> {
        self.auth(self.http.delete(self.url(table)))
            .query(query)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Decides whether a paper entry may be opened. `Err` carries the reason shown to the user.
pub(crate) fn entry_decision(
    auto_enter: bool,
    max_positions: Option<i64>,
    open_count: i64,
    instance_has_open: bool,
) -> Result<(), &'static str> {
    if !auto_enter {
        return Err("auto_entrada_desligada");
    }
    let Some(max) = max_positions else {
        return Err("sem_configuracao_de_risco");
    };
    if instance_has_open {
        return Err("posicao_ja_aberta_no_ativo");
    }
    if open_count >= max {
        return Err("limite_de_posicoes_atingido");
    }
    Ok(())
}

fn pnl_bps(side: &str, entry: f64, exit: f64) -> Option<i64> {
    if !(entry.is_finite() && exit.is_finite()) || entry <= 0.0 || exit <= 0.0 {
        return None;
    }
    let v = match side {
        "BUY" => (exit - entry) / entry * 10_000.0,
        "SELL" => (entry - exit) / entry * 10_000.0,
        _ => return None,
    };
    v.is_finite().then(|| v.round() as i64)
}

fn session_start_for(event_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local = event_time.with_timezone(&New_York);
    let date = local.date_naive();
    let session_date = if local.hour() < 6 { date.pred_opt()? } else { date };
    let start = NaiveDateTime::new(session_date, NaiveTime::from_hms_opt(20, 0, 0)?);
    New_York.from_local_datetime(&start).single().map(|t| t.with_timezone(&Utc))
}

fn seconds_until_next_scan(now_ts: i64) -> u64 {
    let next = (now_ts / 900 + 1) * 900 + SCAN_DELAY_SECONDS;
    (next - now_ts).max(1) as u64
}

fn side_str(side: Side) -> &'static str {
    match side {
        Side::Buy => "BUY",
        Side::Sell => "SELL",
    }
}

struct Found {
    symbol: String,
    event: StrategyEvent,
    entry_candle: Option<Candle>,
}

fn event_parts(event: &StrategyEvent) -> (&'static str, &'static str, f64, DateTime<Utc>, bool, Option<String>) {
    match event {
        StrategyEvent::Enter { side, price, time } => (
            if *side == Side::Buy { "ENTER_BUY" } else { "ENTER_SELL" },
            side_str(*side),
            *price,
            *time,
            true,
            None,
        ),
        StrategyEvent::Exit { side, price, time, reason } => (
            if *side == Side::Buy { "EXIT_BUY" } else { "EXIT_SELL" },
            side_str(*side),
            *price,
            *time,
            false,
            Some(format!("{reason:?}")),
        ),
    }
}

async fn refresh_universe(db: &Db, client: &BybitClient, top_n: usize) -> Result<Vec<TickerSummary>, String> {
    let ranked: Vec<TickerSummary> = client
        .fetch_top_usdt_perpetuals(top_n)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|t| t.symbol.len() <= 24 && t.symbol.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        .collect();
    if ranked.is_empty() {
        return Err("Bybit returned an empty ticker ranking".into());
    }
    let rows: Vec<Value> = ranked
        .iter()
        .enumerate()
        .map(|(i, t)| {
            json!({
                "symbol": t.symbol, "turnover_24h": t.turnover_24h, "last_price": t.last_price,
                "rank": i + 1, "is_active": true, "updated_at": Utc::now().to_rfc3339()
            })
        })
        .collect();
    db.post("scanner_symbols", &[("on_conflict", "symbol".into())], "resolution=merge-duplicates,return=minimal", &Value::Array(rows)).await?;
    let list = ranked.iter().map(|t| t.symbol.as_str()).collect::<Vec<_>>().join(",");
    db.patch("scanner_symbols", &[("symbol", format!("not.in.({list})"))], &json!({"is_active": false})).await?;
    Ok(ranked)
}

async fn store_candles(db: &Db, symbol: &str, candles: &[Candle]) -> Result<(), String> {
    if candles.is_empty() {
        return Ok(());
    }
    let rows: Vec<Value> = candles
        .iter()
        .map(|c| {
            json!({
                "source_venue": "BYBIT", "source_category": "linear", "source_symbol": symbol,
                "timeframe_minutes": 15, "open_time": c.open_time.to_rfc3339(),
                "open": c.open, "high": c.high, "low": c.low, "close": c.close, "volume": c.volume
            })
        })
        .collect();
    db.post(
        "market_candles",
        &[("on_conflict", "source_venue,source_category,source_symbol,timeframe_minutes,open_time".into())],
        "resolution=merge-duplicates,return=minimal",
        &Value::Array(rows),
    )
    .await
    .map(|_| ())
}

async fn scan_symbols(db: &Db, client: &BybitClient, universe: &[TickerSummary], stored: &mut HashMap<String, DateTime<Utc>>) -> Vec<Found> {
    let mut found = Vec::new();
    let now = Utc::now();
    for ticker in universe {
        let candles = match client.fetch_m15(&ticker.symbol, now - Duration::hours(WARMUP_HOURS), now, 200).await {
            Ok(c) if c.len() >= 100 => c,
            Ok(_) => continue,
            Err(error) => {
                debug!(symbol = %ticker.symbol, %error, "Scanner skipped symbol");
                continue;
            }
        };
        let Some(last) = candles.last().map(|c| c.open_time) else { continue };
        if now - (last + Duration::minutes(15)) > Duration::minutes(20) {
            continue; // stale feed for this symbol
        }

        let new_from = stored.get(&ticker.symbol).copied();
        let fresh: Vec<Candle> = candles.iter().copied().filter(|c| new_from.is_none_or(|t| c.open_time > t)).collect();
        match store_candles(db, &ticker.symbol, &fresh).await {
            Ok(()) => {
                stored.insert(ticker.symbol.clone(), last);
            }
            Err(error) => warn!(symbol = %ticker.symbol, %error, "Scanner candle storage failed"),
        }

        let mut engine = FmtEngine::new();
        let mut events = Vec::new();
        let mut failed = false;
        for candle in &candles {
            match engine.on_candle(*candle) {
                Ok(e) => events.extend(e),
                Err(error) => {
                    debug!(symbol = %ticker.symbol, %error, "Scanner engine rejected candle");
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            continue;
        }
        for event in events {
            let (_, _, _, time, _, _) = event_parts(&event);
            if time >= last - Duration::minutes(15) {
                let entry_candle = candles.iter().copied().find(|c| c.open_time == time);
                found.push(Found { symbol: ticker.symbol.clone(), event, entry_candle });
            }
        }
        sleep(StdDuration::from_millis(120)).await;
    }
    found
}

async fn record_opportunities(db: &Db, found: &[Found]) {
    if found.is_empty() {
        return;
    }
    let rows: Vec<Value> = found
        .iter()
        .map(|f| {
            let (event_type, side, price, time, _, reason) = event_parts(&f.event);
            json!({
                "symbol": f.symbol, "event_type": event_type, "side": side,
                "candle_time": time.to_rfc3339(), "reference_price": price,
                "payload": {"origin": "SCANNER", "reason": reason, "timeframe": "M15"}
            })
        })
        .collect();
    if let Err(error) = db
        .post(
            "scanner_opportunities",
            &[("on_conflict", "symbol,event_type,candle_time".into())],
            "resolution=ignore-duplicates,return=minimal",
            &Value::Array(rows),
        )
        .await
    {
        error!(%error, "Could not record scanner opportunities");
    }
}

fn id_list(ids: &HashSet<String>) -> String {
    format!("in.({})", ids.iter().cloned().collect::<Vec<_>>().join(","))
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn num_field(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(|x| x.as_f64().or_else(|| x.as_str()?.parse().ok()))
}

#[derive(Debug, Clone)]
struct OpenPos {
    id: String,
    user_id: String,
    instance_id: String,
    side: String,
    entry_price: f64,
}

/// Fans scanner events out to users and opens/closes paper positions inside their limits.
async fn deliver(db: &Db, found: &[Found], exec: Option<&ExecConfig>) -> Result<(), String> {
    if found.is_empty() {
        return Ok(());
    }
    let symbols: HashSet<String> = found.iter().map(|f| f.symbol.clone()).collect();

    let scanner_users = db.get("scanner_settings", &[("enabled", "eq.true".into()), ("select", "user_id,auto_paper_enter".into())]).await?;
    let manual = db
        .get(
            "strategy_instances",
            &[
                ("strategy_code", "eq.FMT_TOKYO_M15".into()),
                ("is_enabled", "eq.true".into()),
                ("source_symbol", id_list(&symbols)),
                ("select", "id,user_id,source_symbol".into()),
            ],
        )
        .await?;

    let mut users: HashSet<String> = HashSet::new();
    let mut auto_of: HashMap<String, bool> = HashMap::new();
    let mut scanner_set: HashSet<String> = HashSet::new();
    for row in &scanner_users {
        if let Some(u) = str_field(row, "user_id") {
            users.insert(u.to_string());
            scanner_set.insert(u.to_string());
            auto_of.insert(u.to_string(), row.get("auto_paper_enter").and_then(Value::as_bool).unwrap_or(true));
        }
    }
    for row in &manual {
        if let Some(u) = str_field(row, "user_id") {
            users.insert(u.to_string());
            auto_of.entry(u.to_string()).or_insert(true);
        }
    }
    if users.is_empty() {
        return Ok(());
    }

    let instances = db
        .get(
            "strategy_instances",
            &[("strategy_code", "eq.FMT_TOKYO_M15".into()), ("user_id", id_list(&users)), ("select", "id,user_id,source_symbol,is_enabled".into())],
        )
        .await?;
    // (user, symbol) -> (instance id, enabled)
    let mut instance_of: HashMap<(String, String), (String, bool)> = HashMap::new();
    for row in &instances {
        if let (Some(id), Some(u), Some(s)) = (str_field(row, "id"), str_field(row, "user_id"), str_field(row, "source_symbol")) {
            instance_of.insert((u.to_string(), s.to_string()), (id.to_string(), row.get("is_enabled").and_then(Value::as_bool).unwrap_or(false)));
        }
    }

    let risk_rows = db.get("risk_configs", &[("user_id", id_list(&users)), ("select", "user_id,max_open_positions".into())]).await?;
    let mut max_of: HashMap<String, i64> = HashMap::new();
    for row in &risk_rows {
        if let (Some(u), Some(m)) = (str_field(row, "user_id"), row.get("max_open_positions").and_then(Value::as_i64)) {
            max_of.insert(u.to_string(), m);
        }
    }

    let open_rows = db
        .get("paper_positions", &[("user_id", id_list(&users)), ("status", "eq.OPEN".into()), ("select", "id,user_id,strategy_instance_id,side,entry_price".into())])
        .await?;
    let mut open: Vec<OpenPos> = open_rows
        .iter()
        .filter_map(|r| {
            Some(OpenPos {
                id: str_field(r, "id")?.to_string(),
                user_id: str_field(r, "user_id")?.to_string(),
                instance_id: str_field(r, "strategy_instance_id")?.to_string(),
                side: str_field(r, "side")?.to_string(),
                entry_price: num_field(r, "entry_price")?,
            })
        })
        .collect();

    // Exits first, so slots freed on the same candle can be reused by entries.
    let mut ordered: Vec<&Found> = found.iter().collect();
    ordered.sort_by_key(|f| matches!(f.event, StrategyEvent::Enter { .. }));

    for f in ordered {
        let (event_type, side, price, time, is_entry, reason) = event_parts(&f.event);
        for user in &users {
            let is_scanner_user = scanner_set.contains(user);
            let key = (user.clone(), f.symbol.clone());
            let instance_id = match instance_of.get(&key) {
                Some((id, true)) => id.clone(),
                Some((_, false)) => continue, // the user disabled this symbol explicitly
                None if is_scanner_user => {
                    let created = db
                        .post(
                            "strategy_instances",
                            &[("on_conflict", "user_id,strategy_code,source_symbol".into())],
                            "resolution=ignore-duplicates,return=representation",
                            &json!({"user_id": user, "strategy_code": "FMT_TOKYO_M15", "source_symbol": f.symbol, "is_enabled": true}),
                        )
                        .await?;
                    match created.as_array().and_then(|a| a.first()).and_then(|r| str_field(r, "id")) {
                        Some(id) => {
                            instance_of.insert(key, (id.to_string(), true));
                            id.to_string()
                        }
                        None => continue,
                    }
                }
                None => continue,
            };

            let auto = *auto_of.get(user).unwrap_or(&true);
            let mut entered = false;
            let mut skipped: Option<&'static str> = None;

            if is_entry {
                let open_count = open.iter().filter(|p| &p.user_id == user).count() as i64;
                let has_open = open.iter().any(|p| p.instance_id == instance_id);
                match entry_decision(auto, max_of.get(user).copied(), open_count, has_open) {
                    Ok(()) => {
                        db.post(
                            "paper_positions",
                            &[("on_conflict", "strategy_instance_id,entry_candle_open_time".into())],
                            "resolution=ignore-duplicates,return=minimal",
                            &json!({
                                "user_id": user, "strategy_instance_id": instance_id, "side": side,
                                "entry_price": price, "entry_candle_open_time": time.to_rfc3339(),
                                "entry_origin": "SCANNER", "status": "OPEN"
                            }),
                        )
                        .await?;
                        open.push(OpenPos { id: String::new(), user_id: user.clone(), instance_id: instance_id.clone(), side: side.to_string(), entry_price: price });
                        entered = true;

                        // Real order (opt-in): only for the configured operator, after every executor gate.
                        if let (Some(cfg), Some(candle)) = (exec, f.entry_candle) {
                            if &cfg.user_id == user {
                                let buy = side == "BUY";
                                let request = ExecRequest {
                                    user_id: user.clone(),
                                    symbol: f.symbol.clone(),
                                    buy,
                                    entry_ref: price,
                                    stop_raw: if buy { candle.low } else { candle.high },
                                    candle_time: time,
                                    max_positions: max_of.get(user).copied().unwrap_or(0),
                                };
                                match executor::try_execute(cfg, db, &request).await {
                                    Ok(outcome) => info!(symbol = %f.symbol, outcome, "Execution decision"),
                                    Err(error) => error!(symbol = %f.symbol, %error, "Execution failed"),
                                }
                            }
                        }
                    }
                    Err(why) => skipped = Some(why),
                }
            } else if let Some(pos) = open.iter().find(|p| p.instance_id == instance_id && p.side == side).cloned() {
                if !pos.id.is_empty() {
                    if let Some(pnl) = pnl_bps(side, pos.entry_price, price) {
                        db.patch(
                            "paper_positions",
                            &[("id", format!("eq.{}", pos.id)), ("status", "eq.OPEN".into())],
                            &json!({"status": "CLOSED", "exit_price": price, "exit_candle_open_time": time.to_rfc3339(), "pnl_bps": pnl}),
                        )
                        .await?;
                    }
                }
                open.retain(|p| p.id != pos.id || p.id.is_empty());
            }

            db.post(
                "strategy_events",
                &[("on_conflict", "user_id,strategy_instance_id,source_candle_open_time,event_type".into())],
                "resolution=ignore-duplicates,return=minimal",
                &json!({
                    "user_id": user, "strategy_instance_id": instance_id, "event_type": event_type,
                    "source_candle_open_time": time.to_rfc3339(), "reference_price": price,
                    "payload": {
                        "side": side, "symbol": f.symbol, "source": "BYBIT", "timeframe": "M15",
                        "origin": "SCANNER", "reason": reason, "execution_eligible": false,
                        "is_live_signal": true, "auto_entered": entered, "skipped_reason": skipped
                    }
                }),
            )
            .await?;
        }
    }
    Ok(())
}

/// Closes paper positions whose session has ended (the engine resets silently at session change,
/// so they would otherwise hold a slot forever). Exit = close of the last session candle.
async fn close_stale_positions(db: &Db) -> Result<(), String> {
    let rows = db
        .get("paper_positions", &[("status", "eq.OPEN".into()), ("select", "id,side,entry_price,entry_candle_open_time,strategy_instances(source_symbol)".into())])
        .await?;
    let now = Utc::now();
    for row in rows {
        let (Some(id), Some(side), Some(entry), Some(entry_time)) = (
            str_field(&row, "id"),
            str_field(&row, "side"),
            num_field(&row, "entry_price"),
            str_field(&row, "entry_candle_open_time").and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|t| t.with_timezone(&Utc)),
        ) else {
            continue;
        };
        let Some(symbol) = row.get("strategy_instances").and_then(|v| str_field(v, "source_symbol")) else { continue };
        let Some(start) = session_start_for(entry_time) else { continue };
        let end = start + Duration::hours(SESSION_LENGTH_HOURS);
        if now < end + Duration::minutes(20) {
            continue;
        }
        let last_candle = end - Duration::minutes(15);
        let candle = db
            .get("market_candles", &[("source_symbol", format!("eq.{symbol}")), ("open_time", format!("eq.{}", last_candle.to_rfc3339())), ("select", "close".into())])
            .await?;
        let Some(exit) = candle.first().and_then(|c| num_field(c, "close")) else { continue };
        let Some(pnl) = pnl_bps(side, entry, exit) else { continue };
        db.patch(
            "paper_positions",
            &[("id", format!("eq.{id}")), ("status", "eq.OPEN".into())],
            &json!({"status": "CLOSED", "exit_price": exit, "exit_candle_open_time": last_candle.to_rfc3339(), "pnl_bps": pnl}),
        )
        .await?;
        info!(symbol, id, "Closed stale paper position at session end");
    }
    Ok(())
}

pub async fn run(environment: Environment) {
    let Some(db) = Db::from_env() else {
        warn!("Scanner disabled: SUPABASE_URL or SUPABASE_SERVICE_ROLE_KEY missing");
        return;
    };
    let client = match BybitClient::new(environment) {
        Ok(c) => c,
        Err(error) => {
            error!(%error, "Scanner could not create the Bybit client");
            return;
        }
    };
    let top_n = env::var("SCANNER_TOP_N").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(DEFAULT_TOP_N).clamp(1, MAX_TOP_N);
    let exec_cfg = match ExecConfig::from_env() {
        Ok(Some(cfg)) => {
            warn!(environment = cfg.environment_name(), "REAL ORDER EXECUTION IS ENABLED for the configured operator");
            Some(cfg)
        }
        Ok(None) => None,
        Err(reason) => {
            error!(%reason, "Execution misconfigured; staying in paper mode");
            None
        }
    };
    info!(top_n, "Multi-asset scanner started");

    let mut universe: Vec<TickerSummary> = Vec::new();
    let mut universe_at: Option<DateTime<Utc>> = None;
    let mut stored: HashMap<String, DateTime<Utc>> = HashMap::new();
    let mut last_prune = Utc::now() - Duration::days(1);

    loop {
        let now = Utc::now();
        if universe_at.is_none_or(|t| now - t > Duration::minutes(UNIVERSE_REFRESH_MINUTES)) {
            match refresh_universe(&db, &client, top_n).await {
                Ok(list) => {
                    info!(symbols = list.len(), "Scanner universe refreshed");
                    universe = list;
                    universe_at = Some(now);
                }
                Err(error) => warn!(%error, "Scanner universe refresh failed"),
            }
        }

        if !universe.is_empty() {
            let found = scan_symbols(&db, &client, &universe, &mut stored).await;
            info!(symbols = universe.len(), events = found.len(), "Scanner pass complete");
            record_opportunities(&db, &found).await;
            if let Err(error) = deliver(&db, &found, exec_cfg.as_ref()).await {
                error!(%error, "Scanner delivery failed");
            }
            if let Err(error) = close_stale_positions(&db).await {
                warn!(%error, "Stale paper position cleanup failed");
            }
        }

        if now - last_prune > Duration::hours(24) {
            let cutoff = (now - Duration::days(CANDLE_RETENTION_DAYS)).to_rfc3339();
            if let Err(error) = db.delete("market_candles", &[("source_symbol", "neq.BTCUSDT".into()), ("open_time", format!("lt.{cutoff}"))]).await {
                warn!(%error, "Scanner candle pruning failed");
            }
            last_prune = now;
        }

        sleep(StdDuration::from_secs(seconds_until_next_scan(Utc::now().timestamp()))).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_respects_user_limits() {
        assert_eq!(entry_decision(true, Some(2), 1, false), Ok(()));
        assert_eq!(entry_decision(true, Some(2), 2, false), Err("limite_de_posicoes_atingido"));
        assert_eq!(entry_decision(true, Some(2), 3, false), Err("limite_de_posicoes_atingido"));
        assert_eq!(entry_decision(false, Some(2), 0, false), Err("auto_entrada_desligada"));
        assert_eq!(entry_decision(true, None, 0, false), Err("sem_configuracao_de_risco"));
        assert_eq!(entry_decision(true, Some(2), 0, true), Err("posicao_ja_aberta_no_ativo"));
    }

    #[test]
    fn pnl_is_signed_by_side() {
        assert_eq!(pnl_bps("BUY", 100.0, 101.0), Some(100));
        assert_eq!(pnl_bps("SELL", 100.0, 101.0), Some(-100));
        assert_eq!(pnl_bps("BUY", 0.0, 1.0), None);
        assert_eq!(pnl_bps("HOLD", 1.0, 1.0), None);
    }

    #[test]
    fn next_scan_waits_for_candle_close_plus_delay() {
        assert_eq!(seconds_until_next_scan(900 * 10), 900 + SCAN_DELAY_SECONDS as u64);
        assert_eq!(seconds_until_next_scan(900 * 10 + 899), 1 + SCAN_DELAY_SECONDS as u64);
    }

    #[test]
    fn session_start_is_dst_aware() {
        let t = New_York.with_ymd_and_hms(2026, 7, 13, 21, 15, 0).unwrap().with_timezone(&Utc);
        let want = New_York.with_ymd_and_hms(2026, 7, 13, 20, 0, 0).unwrap().with_timezone(&Utc);
        assert_eq!(session_start_for(t), Some(want));
        let overnight = New_York.with_ymd_and_hms(2026, 1, 13, 1, 15, 0).unwrap().with_timezone(&Utc);
        let want2 = New_York.with_ymd_and_hms(2026, 1, 12, 20, 0, 0).unwrap().with_timezone(&Utc);
        assert_eq!(session_start_for(overnight), Some(want2));
    }
}
