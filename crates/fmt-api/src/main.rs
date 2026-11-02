use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use ethers_core::types::{Address, Signature};
use fmt_risk::RiskConfig;
use rand::{rngs::OsRng, RngCore};
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{env, net::SocketAddr, str::FromStr};
use tower_http::cors::{AllowOrigin, CorsLayer};

const GMX_CHAIN_ID: u64 = 42161;

#[derive(Clone)]
struct AppState {
    http: Client,
    supabase_url: String,
    supabase_anon_key: String,
    supabase_service_role_key: String,
    gmx_adapter_url: String,
    gmx_adapter_internal_key: String,
}

#[derive(Debug, Deserialize)]
struct SupabaseUser {
    id: String,
    email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RiskConfigRecord {
    user_id: String,
    #[serde(flatten)]
    config: RiskConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct WalletConnectionRecord {
    wallet_address: String,
    chain_id: u64,
    verified_at: String,
}

#[derive(Debug, Deserialize)]
struct WalletChallengeRecord {
    id: String,
    user_id: String,
    chain_id: u64,
    wallet_address: String,
    nonce: String,
    message: String,
    expires_at: String,
    used_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StrategyToggleRequest {
    enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct StrategyInstanceRecord {
    id: String,
    user_id: String,
    strategy_code: String,
    source_symbol: String,
    is_enabled: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct GmxAccountQuery {
    wallet_address: String,
}

#[derive(Debug, Deserialize)]
struct WalletChallengeRequest {
    wallet_address: String,
    chain_id: u64,
}

#[derive(Debug, Serialize)]
struct WalletChallengeResponse {
    wallet_address: String,
    chain_id: u64,
    nonce: String,
    message: String,
    expires_at: String,
}

#[derive(Debug, Deserialize)]
struct WalletVerifyRequest {
    wallet_address: String,
    chain_id: u64,
    nonce: String,
    signature: String,
}

#[derive(Debug, Serialize)]
struct WalletVerifyResponse {
    verified: bool,
    wallet_address: String,
    chain_id: u64,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    execution_enabled: bool,
}

#[derive(Serialize)]
struct MeResponse {
    user_id: String,
    email: Option<String>,
}

async fn health(State(_state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "fmt-api",
        execution_enabled: false,
    })
}

async fn authenticate_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<SupabaseUser, StatusCode> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.starts_with("Bearer ") && value.len() > 7)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let response = state
        .http
        .get(format!("{}/auth/v1/user", state.supabase_url.trim_end_matches('/')))
        .header("apikey", &state.supabase_anon_key)
        .header(header::AUTHORIZATION, auth_header)
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    if response.status() == StatusCode::UNAUTHORIZED
        || response.status() == StatusCode::FORBIDDEN
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if !response.status().is_success() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let user = response
        .json::<SupabaseUser>()
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    if user.id.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(user)
}

fn admin_request(state: &AppState, request: RequestBuilder) -> RequestBuilder {
    request
        .header("apikey", &state.supabase_service_role_key)
        .bearer_auth(&state.supabase_service_role_key)
}

fn normalize_wallet_address(address: &str) -> Result<String, StatusCode> {
    let parsed = Address::from_str(address).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(format!("{:#x}", parsed).to_lowercase())
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, StatusCode> {
    let user = authenticate_user(&state, &headers).await?;
    Ok(Json(MeResponse {
        user_id: user.id,
        email: user.email,
    }))
}

async fn get_risk_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RiskConfig>, StatusCode> {
    let user = authenticate_user(&state, &headers).await?;
    let url = format!("{}/rest/v1/risk_configs", state.supabase_url.trim_end_matches('/'));
    let user_filter = format!("eq.{}", user.id);
    let rows = admin_request(&state, state.http.get(url))
        .query(&[
            ("select", "user_id,max_open_positions,max_position_notional_cents,max_total_exposure_cents,max_portfolio_leverage_bps,max_price_divergence_bps,min_free_collateral_bps"),
            ("user_id", user_filter.as_str()),
            ("limit", "1"),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .json::<Vec<RiskConfigRecord>>()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let record = rows.into_iter().next().ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(record.config))
}

async fn put_risk_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(config): Json<RiskConfig>,
) -> Result<Json<RiskConfig>, StatusCode> {
    let user = authenticate_user(&state, &headers).await?;
    config.validate().map_err(|_| StatusCode::BAD_REQUEST)?;

    let url = format!("{}/rest/v1/risk_configs", state.supabase_url.trim_end_matches('/'));
    let record = json!({
        "user_id": user.id,
        "max_open_positions": config.max_open_positions,
        "max_position_notional_cents": config.max_position_notional_cents,
        "max_total_exposure_cents": config.max_total_exposure_cents,
        "max_portfolio_leverage_bps": config.max_portfolio_leverage_bps,
        "max_price_divergence_bps": config.max_price_divergence_bps,
        "min_free_collateral_bps": config.min_free_collateral_bps
    });

    admin_request(&state, state.http.post(url))
        .query(&[("on_conflict", "user_id")])
        .header("Prefer", "resolution=merge-duplicates,return=minimal")
        .json(&record)
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(Json(config))
}

async fn get_wallet_connections(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<WalletConnectionRecord>>, StatusCode> {
    let user = authenticate_user(&state, &headers).await?;
    let url = format!("{}/rest/v1/wallet_connections", state.supabase_url.trim_end_matches('/'));
    let user_filter = format!("eq.{}", user.id);
    let rows = admin_request(&state, state.http.get(url))
        .query(&[
            ("select", "wallet_address,chain_id,verified_at"),
            ("user_id", user_filter.as_str()),
            ("order", "verified_at.desc"),
            ("limit", "100"),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .json::<Vec<WalletConnectionRecord>>()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(rows))
}

async fn get_strategy_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<StrategyInstanceRecord>>, StatusCode> {
    let user = authenticate_user(&state, &headers).await?;
    let url = format!("{}/rest/v1/strategy_instances", state.supabase_url.trim_end_matches('/'));
    let user_filter = format!("eq.{}", user.id);
    let rows = admin_request(&state, state.http.get(url))
        .query(&[
            ("select", "id,user_id,strategy_code,source_symbol,is_enabled,created_at,updated_at"),
            ("user_id", user_filter.as_str()),
            ("strategy_code", "eq.FMT_TOKYO_M15"),
            ("source_symbol", "eq.BTCUSDT"),
            ("limit", "1"),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .json::<Vec<StrategyInstanceRecord>>()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(rows))
}

async fn put_strategy_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<StrategyToggleRequest>,
) -> Result<Json<StrategyInstanceRecord>, StatusCode> {
    let user = authenticate_user(&state, &headers).await?;

    if request.enabled {
        let user_filter = format!("eq.{}", user.id);
        let risk_url = format!("{}/rest/v1/risk_configs", state.supabase_url.trim_end_matches('/'));
        let risk_configs = admin_request(&state, state.http.get(risk_url))
            .query(&[
                ("select", "user_id"),
                ("user_id", user_filter.as_str()),
                ("limit", "1"),
            ])
            .send()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
            .error_for_status()
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
            .json::<Vec<serde_json::Value>>()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        if risk_configs.is_empty() {
            return Err(StatusCode::CONFLICT);
        }
    }

    let url = format!("{}/rest/v1/strategy_instances", state.supabase_url.trim_end_matches('/'));
    let record = json!({
        "user_id": user.id,
        "strategy_code": "FMT_TOKYO_M15",
        "source_venue": "BYBIT",
        "source_category": "linear",
        "source_symbol": "BTCUSDT",
        "timeframe_minutes": 15,
        "session_timezone": "America/New_York",
        "pre_session_start": "18:00:00",
        "tokyo_session_start": "20:00:00",
        "tokyo_session_end": "06:00:00",
        "is_enabled": request.enabled
    });
    let rows = admin_request(&state, state.http.post(url))
        .query(&[("on_conflict", "user_id,strategy_code,source_symbol")])
        .header("Prefer", "resolution=merge-duplicates,return=representation")
        .json(&record)
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .json::<Vec<StrategyInstanceRecord>>()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    rows.into_iter().next().map(Json).ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

async fn get_gmx_markets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let _user = authenticate_user(&state, &headers).await?;
    let url = format!("{}/markets", state.gmx_adapter_url.trim_end_matches('/'));
    let response = state
        .http
        .get(url)
        .header("x-internal-api-key", &state.gmx_adapter_internal_key)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !response.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let data = response
        .json::<serde_json::Value>()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Json(data))
}

async fn get_gmx_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GmxAccountQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user = authenticate_user(&state, &headers).await?;
    let wallet_address = normalize_wallet_address(&query.wallet_address)?;
    let user_filter = format!("eq.{}", user.id);
    let address_filter = format!("eq.{}", wallet_address);
    let chain_filter = format!("eq.{}", GMX_CHAIN_ID);
    let wallets_url = format!("{}/rest/v1/wallet_connections", state.supabase_url.trim_end_matches('/'));
    let verified_wallets = admin_request(&state, state.http.get(wallets_url))
        .query(&[
            ("select", "wallet_address,chain_id,verified_at"),
            ("user_id", user_filter.as_str()),
            ("wallet_address", address_filter.as_str()),
            ("chain_id", chain_filter.as_str()),
            ("limit", "1"),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .json::<Vec<WalletConnectionRecord>>()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if verified_wallets.is_empty() {
        return Err(StatusCode::FORBIDDEN);
    }

    let url = format!(
        "{}/account/{}",
        state.gmx_adapter_url.trim_end_matches('/'),
        wallet_address
    );
    let response = state
        .http
        .get(url)
        .header("x-internal-api-key", &state.gmx_adapter_internal_key)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !response.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let data = response
        .json::<serde_json::Value>()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Json(data))
}

async fn create_wallet_challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<WalletChallengeRequest>,
) -> Result<Json<WalletChallengeResponse>, StatusCode> {
    let user = authenticate_user(&state, &headers).await?;
    if request.chain_id != GMX_CHAIN_ID {
        return Err(StatusCode::BAD_REQUEST);
    }
    let wallet_address = normalize_wallet_address(&request.wallet_address)?;
    let user_filter = format!("eq.{}", user.id);
    let address_filter = format!("eq.{}", wallet_address);
    let active_expiry_filter = format!("gt.{}", Utc::now().to_rfc3339());
    let challenge_url = format!("{}/rest/v1/wallet_challenges", state.supabase_url.trim_end_matches('/'));
    let active_challenges = admin_request(&state, state.http.get(challenge_url.as_str()))
        .query(&[
            ("select", "id"),
            ("user_id", user_filter.as_str()),
            ("wallet_address", address_filter.as_str()),
            ("used_at", "is.null"),
            ("expires_at", active_expiry_filter.as_str()),
            ("limit", "5"),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .json::<Vec<serde_json::Value>>()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if active_challenges.len() >= 5 {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let mut nonce_bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = hex::encode(nonce_bytes);
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::minutes(5);
    let expires_at_text = expires_at.to_rfc3339();
    let issued_at_text = issued_at.to_rfc3339();
    let message = format!(
        "FMT QUANTUM - Wallet Ownership Verification\n\nUser ID: {}\nWallet: {}\nChain ID: {}\nNonce: {}\nIssued At: {}\nExpiration Time: {}\n\nPurpose: prove wallet ownership only. This signature does not authorize trading, token approvals, deposits, withdrawals, or delegated access.",
        user.id,
        wallet_address,
        request.chain_id,
        nonce,
        issued_at_text,
        expires_at_text
    );

    let row = json!({
        "user_id": user.id,
        "chain_id": request.chain_id,
        "wallet_address": wallet_address,
        "nonce": nonce,
        "message": message,
        "expires_at": expires_at_text
    });
    admin_request(&state, state.http.post(challenge_url.as_str()))
        .json(&row)
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(Json(WalletChallengeResponse {
        wallet_address,
        chain_id: request.chain_id,
        nonce,
        message,
        expires_at: expires_at_text,
    }))
}

async fn verify_wallet_signature(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<WalletVerifyRequest>,
) -> Result<Json<WalletVerifyResponse>, StatusCode> {
    let user = authenticate_user(&state, &headers).await?;
    if request.chain_id != GMX_CHAIN_ID
        || request.nonce.len() != 64
        || request.signature.len() != 132
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let wallet_address = normalize_wallet_address(&request.wallet_address)?;
    let nonce_filter = format!("eq.{}", request.nonce);
    let user_filter = format!("eq.{}", user.id);
    let address_filter = format!("eq.{}", wallet_address);
    let chain_filter = format!("eq.{}", request.chain_id);
    let challenge_url = format!("{}/rest/v1/wallet_challenges", state.supabase_url.trim_end_matches('/'));

    let rows = admin_request(&state, state.http.get(&challenge_url))
        .query(&[
            ("select", "id,user_id,chain_id,wallet_address,nonce,message,expires_at,used_at"),
            ("user_id", user_filter.as_str()),
            ("wallet_address", address_filter.as_str()),
            ("chain_id", chain_filter.as_str()),
            ("nonce", nonce_filter.as_str()),
            ("used_at", "is.null"),
            ("limit", "1"),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .json::<Vec<WalletChallengeRecord>>()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let challenge = rows.into_iter().next().ok_or(StatusCode::CONFLICT)?;
    if challenge.user_id != user.id
        || challenge.wallet_address != wallet_address
        || challenge.chain_id != request.chain_id
        || challenge.nonce != request.nonce
        || challenge.used_at.is_some()
    {
        return Err(StatusCode::CONFLICT);
    }
    let expiry = DateTime::parse_from_rfc3339(&challenge.expires_at)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if expiry <= Utc::now() {
        return Err(StatusCode::GONE);
    }

    let signature = Signature::from_str(&request.signature)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let recovered = signature
        .recover(challenge.message.as_str())
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let recovered_address = format!("{:#x}", recovered).to_lowercase();
    if recovered_address != wallet_address {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Atomically consume the challenge. Only one concurrent verification can
    // change used_at from NULL, preventing signature replay.
    let now = Utc::now();
    let now_text = now.to_rfc3339();
    let expires_filter = format!("gt.{}", now_text);
    let challenge_id_filter = format!("eq.{}", challenge.id);
    let update = json!({"used_at": now_text});
    let consumed = admin_request(&state, state.http.patch(challenge_url.as_str()))
        .query(&[
            ("id", challenge_id_filter.as_str()),
            ("user_id", user_filter.as_str()),
            ("used_at", "is.null"),
            ("expires_at", expires_filter.as_str()),
        ])
        .header("Prefer", "return=representation")
        .json(&update)
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .json::<Vec<WalletChallengeRecord>>()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if consumed.is_empty() {
        return Err(StatusCode::CONFLICT);
    }

    let connection_url = format!("{}/rest/v1/wallet_connections", state.supabase_url.trim_end_matches('/'));
    let connection = json!({
        "user_id": user.id,
        "chain_id": request.chain_id,
        "wallet_address": wallet_address,
        "verified_at": now_text
    });
    admin_request(&state, state.http.post(connection_url))
        .query(&[("on_conflict", "user_id,chain_id,wallet_address")])
        .header("Prefer", "resolution=merge-duplicates,return=minimal")
        .json(&connection)
        .send()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .error_for_status()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(Json(WalletVerifyResponse {
        verified: true,
        wallet_address,
        chain_id: request.chain_id,
    }))
}

#[tokio::main]
async fn main() {
    let supabase_url =
        env::var("SUPABASE_URL").expect("SUPABASE_URL must be configured");
    let supabase_anon_key =
        env::var("SUPABASE_ANON_KEY").expect("SUPABASE_ANON_KEY must be configured");
    let supabase_service_role_key = env::var("SUPABASE_SERVICE_ROLE_KEY")
        .expect("SUPABASE_SERVICE_ROLE_KEY must be configured on the server only");
    let gmx_adapter_url = env::var("GMX_ADAPTER_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string());
    let gmx_adapter_internal_key = env::var("GMX_ADAPTER_INTERNAL_KEY")
        .expect("GMX_ADAPTER_INTERNAL_KEY must be configured on the server only");
    let bind = env::var("API_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let allowed_origin =
        env::var("CORS_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let origin = HeaderValue::from_str(&allowed_origin)
        .expect("CORS_ORIGIN must be a valid origin URL");
    let state = AppState {
        http: Client::new(),
        supabase_url,
        supabase_anon_key,
        supabase_service_role_key,
        gmx_adapter_url,
        gmx_adapter_internal_key,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/me", get(me))
        .route(
            "/v1/risk-config",
            get(get_risk_config).put(put_risk_config),
        )
        .route("/v1/strategy", get(get_strategy_instance).put(put_strategy_instance))
        .route("/v1/gmx/markets", get(get_gmx_markets))
        .route("/v1/gmx/account", get(get_gmx_account))
        .route("/v1/wallets", get(get_wallet_connections))
        .route("/v1/wallet/challenge", post(create_wallet_challenge))
        .route("/v1/wallet/verify", post(verify_wallet_signature))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact(origin))
                .allow_methods([Method::GET, Method::PUT, Method::POST])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        );

    let address: SocketAddr = bind.parse().expect("API_BIND must be a valid socket address");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind API listener");
    axum::serve(listener, app).await.expect("API server failed");
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_wallet_addresses_to_lowercase() {
        let address = normalize_wallet_address("0x2c7536E3605D9C16a7a3D7b1898e529396a65c23")
            .unwrap();
        assert_eq!(address, "0x2c7536e3605d9c16a7a3d7b1898e529396a65c23");
    }

    #[test]
    fn recovers_personal_sign_message_address() {
        let signature = Signature::from_str(
            "0xb91467e570a6466aa9e9876cbcd013baba02900b8979d43fe208a4a4f339f5fd6007e74cd82e037b800186422fc2da167c747ef045e5d18a5f5d4300f8e1a0291c",
        )
        .unwrap();
        let recovered = signature.recover("Some data").unwrap();
        assert_eq!(
            format!("{:#x}", recovered),
            "0x2c7536e3605d9c16a7a3d7b1898e529396a65c23"
        );
    }
}
