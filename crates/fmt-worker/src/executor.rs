//! Bybit V5 order executor (opt-in, fail-closed).
//!
//! Real orders are created ONLY when ALL of these hold:
//!   * EXECUTION_ENABLED=true and valid BYBIT_API_KEY / BYBIT_API_SECRET,
//!   * the signal belongs to EXECUTION_USER_ID (single operator account),
//!   * EXECUTION_ENV=testnet (default) — or EXECUTION_ENV=production together with
//!     EXECUTION_MAINNET_CONFIRM=SIM_DINHEIRO_REAL,
//!   * the account is in ISOLATED margin mode, every gate in `plan_order` passes,
//!     the daily loss limit is not hit and the position limit is not reached.
//!
//! Every order carries a mandatory stop loss and a take profit (bracket, Full mode).

use crate::scanner::Db;
use chrono::{DateTime, Duration, Timelike, Utc};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde_json::{json, Value};
use sha2::Sha256;
use std::{env, time::Duration as StdDuration};
use tracing::{info, warn};

const RECV_WINDOW: &str = "5000";
const MAINNET_CONFIRMATION: &str = "SIM_DINHEIRO_REAL";
const REWARD_MULTIPLE: f64 = 3.0;

#[derive(Debug, Clone)]
pub struct ExecConfig {
    api_key: String,
    api_secret: String,
    pub production: bool,
    pub user_id: String,
    pub risk_usd: f64,
    pub max_leverage: f64,
    pub max_equity_risk_pct: f64,
    pub max_daily_loss_usd: f64,
    pub min_stop_bps: f64,
    pub max_stop_bps: f64,
}

impl ExecConfig {
    /// `Ok(None)` = execution off. `Err` = misconfigured (stays off, reason is logged).
    pub fn from_env() -> Result<Option<Self>, String> {
        if env::var("EXECUTION_ENABLED").map(|v| v != "true").unwrap_or(true) {
            return Ok(None);
        }
        let num = |name: &str, default: f64| -> Result<f64, String> {
            match env::var(name) {
                Ok(v) => v.parse::<f64>().ok().filter(|x| x.is_finite() && *x > 0.0).ok_or_else(|| format!("{name} must be a positive number")),
                Err(_) => Ok(default),
            }
        };
        let api_key = env::var("BYBIT_API_KEY").map_err(|_| "BYBIT_API_KEY missing".to_string())?;
        let api_secret = env::var("BYBIT_API_SECRET").map_err(|_| "BYBIT_API_SECRET missing".to_string())?;
        let user_id = env::var("EXECUTION_USER_ID").map_err(|_| "EXECUTION_USER_ID missing".to_string())?;
        if user_id.len() != 36 {
            return Err("EXECUTION_USER_ID must be the user's UUID".into());
        }
        let production = match env::var("EXECUTION_ENV").unwrap_or_else(|_| "testnet".into()).as_str() {
            "testnet" => false,
            "production" => true,
            other => return Err(format!("EXECUTION_ENV must be testnet or production, got {other}")),
        };
        if production && env::var("EXECUTION_MAINNET_CONFIRM").ok().as_deref() != Some(MAINNET_CONFIRMATION) {
            return Err("EXECUTION_ENV=production requires EXECUTION_MAINNET_CONFIRM=SIM_DINHEIRO_REAL".into());
        }
        let risk_usd = num("RISK_PER_TRADE_USD", 10.0)?;
        let max_leverage = num("EXECUTION_MAX_LEVERAGE", 3.0)?;
        if max_leverage > 10.0 {
            return Err("EXECUTION_MAX_LEVERAGE above 10 is not allowed".into());
        }
        Ok(Some(Self {
            api_key,
            api_secret,
            production,
            user_id,
            risk_usd,
            max_leverage,
            max_equity_risk_pct: num("EXECUTION_MAX_EQUITY_RISK_PCT", 1.0)?,
            max_daily_loss_usd: num("EXECUTION_MAX_DAILY_LOSS_USD", risk_usd * 3.0)?,
            min_stop_bps: num("EXECUTION_MIN_STOP_BPS", 15.0)?,
            max_stop_bps: num("EXECUTION_MAX_STOP_BPS", 500.0)?,
        }))
    }

    fn base(&self) -> &'static str {
        if self.production { "https://api.bybit.com" } else { "https://api-testnet.bybit.com" }
    }

    pub fn environment_name(&self) -> &'static str {
        if self.production { "production" } else { "testnet" }
    }
}

pub struct ExecRequest {
    pub user_id: String,
    pub symbol: String,
    pub buy: bool,
    pub entry_ref: f64,
    pub stop_raw: f64,
    pub candle_time: DateTime<Utc>,
    pub max_positions: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstrumentRules {
    pub qty_step: f64,
    pub min_qty: f64,
    pub max_qty: f64,
    pub tick: f64,
    pub max_leverage: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderPlan {
    pub qty: f64,
    pub qty_text: String,
    pub stop_text: String,
    pub take_profit_text: String,
    pub leverage: f64,
    pub risk_usd: f64,
    pub notional: f64,
    pub stop_bps: f64,
}

fn decimals(step: f64) -> usize {
    let text = format!("{step:.10}");
    let trimmed = text.trim_end_matches('0');
    trimmed.split('.').nth(1).map_or(0, str::len)
}

fn floor_to_step(value: f64, step: f64) -> f64 {
    ((value / step) + 1e-9).floor() * step
}

fn ceil_to_step(value: f64, step: f64) -> f64 {
    ((value / step) - 1e-9).ceil() * step
}

fn fmt_step(value: f64, step: f64) -> String {
    format!("{:.*}", decimals(step), value)
}

/// Pure position-sizing + safety gates (risk-first, per the project's risk rules).
pub fn plan_order(
    cfg: &ExecConfig,
    rules: &InstrumentRules,
    equity: f64,
    buy: bool,
    entry: f64,
    stop: f64,
) -> Result<OrderPlan, &'static str> {
    if !(entry.is_finite() && stop.is_finite() && equity.is_finite()) || entry <= 0.0 || equity <= 0.0 {
        return Err("dados_invalidos");
    }
    if (buy && stop >= entry) || (!buy && stop <= entry) {
        return Err("stop_do_lado_errado");
    }
    let dist = (entry - stop).abs();
    let stop_bps = dist / entry * 10_000.0;
    if stop_bps < cfg.min_stop_bps {
        return Err("stop_curto_demais");
    }
    if stop_bps > cfg.max_stop_bps {
        return Err("stop_largo_demais");
    }
    let leverage = cfg.max_leverage.min(rules.max_leverage);
    if (dist / entry) * leverage > 0.5 {
        return Err("stop_proximo_da_liquidacao");
    }
    let risk = cfg.risk_usd.min(equity * cfg.max_equity_risk_pct / 100.0);
    let by_risk = risk / dist;
    let by_leverage = equity * leverage / entry;
    let qty = floor_to_step(by_risk.min(by_leverage).min(rules.max_qty), rules.qty_step);
    if qty < rules.min_qty {
        return Err("quantidade_abaixo_do_minimo");
    }
    let (stop_px, tp_px) = if buy {
        (floor_to_step(stop, rules.tick), floor_to_step(entry + REWARD_MULTIPLE * dist, rules.tick))
    } else {
        (ceil_to_step(stop, rules.tick), ceil_to_step(entry - REWARD_MULTIPLE * dist, rules.tick))
    };
    if tp_px <= 0.0 || stop_px <= 0.0 {
        return Err("preco_invalido_apos_arredondamento");
    }
    Ok(OrderPlan {
        qty,
        qty_text: fmt_step(qty, rules.qty_step),
        stop_text: fmt_step(stop_px, rules.tick),
        take_profit_text: fmt_step(tp_px, rules.tick),
        leverage,
        risk_usd: qty * dist,
        notional: qty * entry,
        stop_bps,
    })
}

/// Bybit V5 signature: HMAC_SHA256(secret, timestamp + apiKey + recvWindow + payload), hex.
pub fn sign(secret: &str, timestamp: i64, api_key: &str, recv_window: &str, payload: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(format!("{timestamp}{api_key}{recv_window}{payload}").as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn order_link_id(symbol: &str, candle_time: DateTime<Utc>, buy: bool) -> String {
    format!("fq-{}-{}-{}", symbol, candle_time.timestamp(), if buy { "B" } else { "S" })
}

pub fn order_body(symbol: &str, buy: bool, plan: &OrderPlan, link_id: &str) -> Value {
    json!({
        "category": "linear",
        "symbol": symbol,
        "side": if buy { "Buy" } else { "Sell" },
        "orderType": "Market",
        "qty": plan.qty_text,
        "positionIdx": 0,
        "reduceOnly": false,
        "stopLoss": plan.stop_text,
        "takeProfit": plan.take_profit_text,
        "tpslMode": "Full",
        "slTriggerBy": "MarkPrice",
        "tpTriggerBy": "LastPrice",
        "orderLinkId": link_id
    })
}

struct Bybit<'a> {
    http: Client,
    cfg: &'a ExecConfig,
}

impl<'a> Bybit<'a> {
    fn new(cfg: &'a ExecConfig) -> Result<Self, String> {
        Ok(Self { http: Client::builder().timeout(StdDuration::from_secs(15)).build().map_err(|e| e.to_string())?, cfg })
    }

    fn headers(&self, rb: reqwest::RequestBuilder, payload: &str) -> reqwest::RequestBuilder {
        let ts = Utc::now().timestamp_millis();
        rb.header("X-BAPI-API-KEY", &self.cfg.api_key)
            .header("X-BAPI-TIMESTAMP", ts.to_string())
            .header("X-BAPI-RECV-WINDOW", RECV_WINDOW)
            .header("X-BAPI-SIGN", sign(&self.cfg.api_secret, ts, &self.cfg.api_key, RECV_WINDOW, payload))
    }

    async fn finish(resp: reqwest::Response) -> Result<(i64, String, Value), String> {
        let value: Value = resp.json().await.map_err(|e| e.to_string())?;
        let code = value.get("retCode").and_then(Value::as_i64).ok_or("missing retCode")?;
        let msg = value.get("retMsg").and_then(Value::as_str).unwrap_or("").to_string();
        Ok((code, msg, value.get("result").cloned().unwrap_or(Value::Null)))
    }

    async fn get(&self, path: &str, query: &str) -> Result<Value, String> {
        let url = format!("{}{}?{}", self.cfg.base(), path, query);
        let resp = self.headers(self.http.get(url), query).send().await.map_err(|e| e.to_string())?;
        let (code, msg, result) = Self::finish(resp).await?;
        if code == 0 { Ok(result) } else { Err(format!("Bybit {code}: {msg}")) }
    }

    async fn post(&self, path: &str, body: &Value) -> Result<(i64, String, Value), String> {
        let text = body.to_string();
        let url = format!("{}{}", self.cfg.base(), path);
        let resp = self
            .headers(self.http.post(url), &text)
            .header("Content-Type", "application/json")
            .body(text)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::finish(resp).await
    }
}

fn f(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(|x| x.as_str().and_then(|s| s.parse().ok()).or_else(|| x.as_f64()))
}

fn rules_from(result: &Value) -> Option<InstrumentRules> {
    let item = result.get("list")?.as_array()?.first()?;
    let lot = item.get("lotSizeFilter")?;
    Some(InstrumentRules {
        qty_step: f(lot, "qtyStep")?,
        min_qty: f(lot, "minOrderQty")?,
        max_qty: f(lot, "maxMktOrderQty").or_else(|| f(lot, "maxOrderQty"))?,
        tick: f(item.get("priceFilter")?, "tickSize")?,
        max_leverage: f(item.get("leverageFilter")?, "maxLeverage")?,
    })
}

async fn mark_row(db: &Db, id: &str, status: &str, reason: Option<&str>, extra: Value) {
    let mut body = json!({"status": status, "reason": reason});
    if let (Some(obj), Some(more)) = (body.as_object_mut(), extra.as_object()) {
        for (k, v) in more {
            obj.insert(k.clone(), v.clone());
        }
    }
    if let Err(error) = db.patch("bybit_orders", &[("id", format!("eq.{id}"))], &body).await {
        warn!(%error, "Could not update bybit_orders audit row");
    }
}

/// Runs every gate and, if all pass, sends one bracket market order. Always leaves an audit row.
pub async fn try_execute(cfg: &ExecConfig, db: &Db, req: &ExecRequest) -> Result<&'static str, String> {
    if req.user_id != cfg.user_id {
        return Ok("usuario_sem_execucao");
    }
    let link = order_link_id(&req.symbol, req.candle_time, req.buy);
    if link.len() > 36 {
        return Ok("order_link_id_longo_demais");
    }
    let created = db
        .post(
            "bybit_orders",
            &[("on_conflict", "order_link_id".into())],
            "resolution=ignore-duplicates,return=representation",
            &json!({
                "user_id": req.user_id, "symbol": req.symbol, "side": if req.buy { "BUY" } else { "SELL" },
                "order_link_id": link, "environment": cfg.environment_name(), "status": "PENDING",
                "reference_price": req.entry_ref, "signal_candle_time": req.candle_time.to_rfc3339(),
                "profile": "FMT_SWEEP_STOP_3R"
            }),
        )
        .await?;
    let Some(row_id) = created.as_array().and_then(|a| a.first()).and_then(|r| r.get("id")).and_then(Value::as_str).map(str::to_string) else {
        return Ok("ja_processado");
    };

    let api = Bybit::new(cfg)?;
    macro_rules! block {
        ($reason:expr) => {{
            mark_row(db, &row_id, "BLOCKED", Some($reason), json!({})).await;
            return Ok($reason);
        }};
    }
    macro_rules! fail {
        ($why:expr) => {{
            let why: String = $why;
            mark_row(db, &row_id, "FAILED", Some(&why), json!({})).await;
            return Err(why);
        }};
    }

    // 1) Isolated margin is mandatory (cross margin can spill losses across the account).
    match api.get("/v5/account/info", "").await {
        Ok(info) => {
            if info.get("marginMode").and_then(Value::as_str) != Some("ISOLATED_MARGIN") {
                block!("margem_nao_isolada");
            }
        }
        Err(e) => fail!(e),
    }

    // 2) Equity.
    let equity = match api.get("/v5/account/wallet-balance", "accountType=UNIFIED").await {
        Ok(r) => r.get("list").and_then(|l| l.get(0)).and_then(|a| f(a, "totalEquity")),
        Err(e) => fail!(e),
    };
    let Some(equity) = equity else { block!("equity_indisponivel") };

    // 3) Daily realised loss limit (UTC day).
    let day_start = Utc::now().with_hour(0).and_then(|t| t.with_minute(0)).and_then(|t| t.with_second(0)).unwrap_or_else(|| Utc::now() - Duration::hours(24));
    let pnl_q = format!("category=linear&startTime={}&limit=100", day_start.timestamp_millis());
    match api.get("/v5/position/closed-pnl", &pnl_q).await {
        Ok(r) => {
            let sum: f64 = r.get("list").and_then(Value::as_array).map(|l| l.iter().filter_map(|x| f(x, "closedPnl")).sum()).unwrap_or(0.0);
            if sum <= -cfg.max_daily_loss_usd {
                block!("limite_de_perda_diaria");
            }
        }
        Err(e) => fail!(e),
    }

    // 4) Position limits (user's own limit + one position per symbol).
    match api.get("/v5/position/list", "category=linear&settleCoin=USDT&limit=200").await {
        Ok(r) => {
            let open: Vec<&Value> = r.get("list").and_then(Value::as_array).map(|l| l.iter().filter(|p| f(p, "size").unwrap_or(0.0) > 0.0).collect()).unwrap_or_default();
            if open.iter().any(|p| p.get("symbol").and_then(Value::as_str) == Some(req.symbol.as_str())) {
                block!("posicao_ja_aberta_no_ativo");
            }
            if open.len() as i64 >= req.max_positions {
                block!("limite_de_posicoes_atingido");
            }
        }
        Err(e) => fail!(e),
    }

    // 5) Instrument rules + sizing.
    let rules = match api.get("/v5/market/instruments-info", &format!("category=linear&symbol={}", req.symbol)).await {
        Ok(r) => rules_from(&r),
        Err(e) => fail!(e),
    };
    let Some(rules) = rules else { block!("ativo_indisponivel_no_ambiente") };
    let plan = match plan_order(cfg, &rules, equity, req.buy, req.entry_ref, req.stop_raw) {
        Ok(p) => p,
        Err(reason) => block!(reason),
    };

    // 6) Leverage (code 110043 = already set, fine).
    let lev = plan.leverage.to_string();
    match api.post("/v5/position/set-leverage", &json!({"category": "linear", "symbol": req.symbol, "buyLeverage": lev, "sellLeverage": lev})).await {
        Ok((code, _, _)) if code == 0 || code == 110043 => {}
        Ok((code, msg, _)) => fail!(format!("set-leverage {code}: {msg}")),
        Err(e) => fail!(e),
    }

    // 7) Order (market, with mandatory SL/TP bracket).
    let body = order_body(&req.symbol, req.buy, &plan, &link);
    let plan_json = json!({
        "qty": plan.qty_text, "stop_loss": plan.stop_text, "take_profit": plan.take_profit_text,
        "leverage": plan.leverage, "risk_usd": plan.risk_usd, "notional": plan.notional, "stop_bps": plan.stop_bps, "equity": equity
    });
    match api.post("/v5/order/create", &body).await {
        Ok((0, _, result)) => {
            let order_id = result.get("orderId").and_then(Value::as_str).unwrap_or("");
            mark_row(db, &row_id, "SUBMITTED", None, json!({"bybit_order_id": order_id, "plan": plan_json, "response": result})).await;
            info!(symbol = %req.symbol, order_id, env = cfg.environment_name(), "Bybit bracket order submitted");
            Ok("enviada")
        }
        Ok((code, msg, result)) => {
            mark_row(db, &row_id, "REJECTED", Some(&format!("{code}: {msg}")), json!({"plan": plan_json, "response": result})).await;
            Err(format!("order rejected {code}: {msg}"))
        }
        Err(e) => fail!(e),
    }
}

impl ExecConfig {
    /// Read-only mirror of the operator's Bybit account (balance, positions, daily PnL).
    /// Needs the API keys and the operator's user id; never places orders and ignores EXECUTION_ENABLED.
    pub fn from_env_readonly() -> Option<Self> {
        let api_key = env::var("BYBIT_API_KEY").ok()?;
        let api_secret = env::var("BYBIT_API_SECRET").ok()?;
        let user_id = env::var("EXECUTION_USER_ID").ok().filter(|v| v.len() == 36)?;
        let production = env::var("EXECUTION_ENV").map(|v| v == "production").unwrap_or(false);
        Some(Self {
            api_key,
            api_secret,
            production,
            user_id,
            risk_usd: 0.0,
            max_leverage: 1.0,
            max_equity_risk_pct: 0.0,
            max_daily_loss_usd: 0.0,
            min_stop_bps: 0.0,
            max_stop_bps: 0.0,
        })
    }
}

async fn snapshot(cfg: &ExecConfig) -> Result<Value, String> {
    let api = Bybit::new(cfg)?;
    let info = api.get("/v5/account/info", "").await?;
    let wallet = api.get("/v5/account/wallet-balance", "accountType=UNIFIED").await?;
    let account = wallet.get("list").and_then(|l| l.get(0)).cloned().unwrap_or(Value::Null);
    let positions = api.get("/v5/position/list", "category=linear&settleCoin=USDT&limit=200").await?;
    let day_start = Utc::now().with_hour(0).and_then(|t| t.with_minute(0)).and_then(|t| t.with_second(0)).unwrap_or_else(|| Utc::now() - Duration::hours(24));
    let pnl = api.get("/v5/position/closed-pnl", &format!("category=linear&startTime={}&limit=100", day_start.timestamp_millis())).await?;
    let daily: f64 = pnl.get("list").and_then(Value::as_array).map(|l| l.iter().filter_map(|x| f(x, "closedPnl")).sum()).unwrap_or(0.0);
    let open: Vec<Value> = positions
        .get("list")
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .filter(|p| f(p, "size").unwrap_or(0.0) > 0.0)
                .map(|p| {
                    json!({
                        "symbol": p.get("symbol"), "side": p.get("side"), "size": f(p, "size"), "entry": f(p, "avgPrice"),
                        "mark": f(p, "markPrice"), "unrealized": f(p, "unrealisedPnl"), "liq": f(p, "liqPrice"),
                        "leverage": f(p, "leverage"), "stop_loss": f(p, "stopLoss"), "take_profit": f(p, "takeProfit")
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({
        "environment": cfg.environment_name(),
        "equity": f(&account, "totalEquity"),
        "available": f(&account, "totalAvailableBalance"),
        "unrealized_pnl": f(&account, "totalPerpUPL"),
        "daily_realized_pnl": daily,
        "margin_mode": info.get("marginMode"),
        "positions": open,
        "error": Value::Null,
        "updated_at": Utc::now().to_rfc3339()
    }))
}

/// Keeps `bybit_account_snapshots` fresh so the dashboard shows the REAL account (no simulation).
pub async fn account_sync_loop(cfg: ExecConfig, db: Db) {
    info!(environment = cfg.environment_name(), "Bybit account mirror started (read-only)");
    loop {
        let mut row = match snapshot(&cfg).await {
            Ok(row) => row,
            Err(error) => {
                warn!(%error, "Bybit account snapshot failed");
                json!({"environment": cfg.environment_name(), "error": error, "updated_at": Utc::now().to_rfc3339()})
            }
        };
        if let Some(obj) = row.as_object_mut() {
            obj.insert("user_id".into(), json!(cfg.user_id));
        }
        if let Err(error) = db
            .post("bybit_account_snapshots", &[("on_conflict", "user_id".into())], "resolution=merge-duplicates,return=minimal", &row)
            .await
        {
            warn!(%error, "Could not store the Bybit account snapshot");
        }
        tokio::time::sleep(StdDuration::from_secs(45)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ExecConfig {
        ExecConfig {
            api_key: "k".into(), api_secret: "s".into(), production: false, user_id: "0".repeat(36),
            risk_usd: 100.0, max_leverage: 3.0, max_equity_risk_pct: 1.0, max_daily_loss_usd: 300.0,
            min_stop_bps: 15.0, max_stop_bps: 500.0,
        }
    }

    fn btc() -> InstrumentRules {
        InstrumentRules { qty_step: 0.001, min_qty: 0.001, max_qty: 100.0, tick: 0.1, max_leverage: 100.0 }
    }

    #[test]
    fn hmac_sha256_matches_rfc4231_vector() {
        // RFC 4231 test case 2 (key "Jefe"): validates the HMAC primitive used for Bybit signing.
        let mut mac = Hmac::<Sha256>::new_from_slice(b"Jefe").unwrap();
        mac.update(b"what do ya want for nothing?");
        assert_eq!(hex::encode(mac.finalize().into_bytes()), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[test]
    fn signature_covers_timestamp_key_window_and_payload() {
        let a = sign("secret", 1700000000000, "key", "5000", "category=linear");
        let b = sign("secret", 1700000000001, "key", "5000", "category=linear");
        let c = sign("secret", 1700000000000, "key", "5000", "category=inverse");
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, sign("secret", 1700000000000, "key", "5000", "category=linear"));
    }

    #[test]
    fn sizing_is_risk_first_and_leverage_capped() {
        // equity 10k, risk = min(100, 1% of 10k = 100) = 100; stop 0.3% => by_risk = 100/300 = 0.333 BTC
        let p = plan_order(&cfg(), &btc(), 10_000.0, true, 100_000.0, 99_700.0).unwrap();
        assert_eq!(p.qty_text, "0.300"); // capped by leverage: 10k*3/100k = 0.3 BTC
        assert!((p.risk_usd - 90.0).abs() < 1e-6);
        assert_eq!(p.stop_text, "99700.0");
        assert_eq!(p.take_profit_text, "100900.0"); // +3R
    }

    #[test]
    fn short_side_rounds_stop_up_and_target_down_distance() {
        let p = plan_order(&cfg(), &btc(), 50_000.0, false, 100_000.0, 100_300.0).unwrap();
        assert_eq!(p.stop_text, "100300.0");
        assert_eq!(p.take_profit_text, "99100.0");
    }

    #[test]
    fn gates_block_unsafe_orders() {
        let c = cfg();
        let r = btc();
        assert_eq!(plan_order(&c, &r, 10_000.0, true, 100_000.0, 100_100.0), Err("stop_do_lado_errado"));
        assert_eq!(plan_order(&c, &r, 10_000.0, true, 100_000.0, 99_990.0), Err("stop_curto_demais"));
        assert_eq!(plan_order(&c, &r, 10_000.0, true, 100_000.0, 90_000.0), Err("stop_largo_demais"));
        assert_eq!(plan_order(&c, &r, 5.0, true, 100_000.0, 99_700.0), Err("quantidade_abaixo_do_minimo"));
        let mut hi = c.clone();
        hi.max_leverage = 10.0;
        hi.max_stop_bps = 2_000.0;
        assert_eq!(plan_order(&hi, &r, 10_000.0, true, 100_000.0, 94_000.0), Err("stop_proximo_da_liquidacao"));
        assert_eq!(plan_order(&c, &r, f64::NAN, true, 100_000.0, 99_700.0), Err("dados_invalidos"));
    }

    #[test]
    fn step_formatting_and_link_id() {
        assert_eq!(fmt_step(0.3, 0.001), "0.300");
        assert_eq!(fmt_step(12.0, 1.0), "12");
        assert_eq!(decimals(0.0001), 4);
        let t = DateTime::from_timestamp(1_793_745_900, 0).unwrap();
        let id = order_link_id("BTCUSDT", t, true);
        assert_eq!(id, "fq-BTCUSDT-1793745900-B");
        assert!(id.len() <= 36);
    }

    #[test]
    fn order_body_always_carries_stop_and_target() {
        let p = plan_order(&cfg(), &btc(), 10_000.0, true, 100_000.0, 99_700.0).unwrap();
        let body = order_body("BTCUSDT", true, &p, "fq-x");
        assert_eq!(body["stopLoss"], "99700.0");
        assert_eq!(body["takeProfit"], "100900.0");
        assert_eq!(body["tpslMode"], "Full");
        assert_eq!(body["reduceOnly"], false);
    }

    #[test]
    fn parses_instrument_rules() {
        let v = json!({"list":[{"lotSizeFilter":{"qtyStep":"0.001","minOrderQty":"0.001","maxOrderQty":"1190","maxMktOrderQty":"500"},
            "priceFilter":{"tickSize":"0.10"},"leverageFilter":{"maxLeverage":"100.00"}}]});
        let r = rules_from(&v).unwrap();
        assert_eq!(r, InstrumentRules { qty_step: 0.001, min_qty: 0.001, max_qty: 500.0, tick: 0.1, max_leverage: 100.0 });
    }
}
