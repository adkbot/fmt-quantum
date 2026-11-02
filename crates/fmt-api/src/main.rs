use axum::{
    extract::State,
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
    let url = format!("{}/rest/v1/wallet_challenges", state.supabase_url.trim_end_matches('/'));
    admin_request(&state, state.http.post(url))
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
    let update = json!({"used_at": now_text});
    let consumed = admin_request(&state, state.http.patch(&challenge_url))
        .query(&[
            ("id", format!("eq.{}", challenge.id).as_str()),
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
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/me", get(me))
        .route(
            "/v1/risk-config",
            get(get_risk_config).put(put_risk_config),
        )
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
