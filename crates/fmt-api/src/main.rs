use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    routing::get,
    Json, Router,
};
use fmt_risk::RiskConfig;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{env, net::SocketAddr, str::FromStr};
use tower_http::cors::{AllowOrigin, CorsLayer};

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
            ("select", "user_id,max_open_positions,max_position_notional_usd,max_total_exposure_usd,max_portfolio_leverage,max_price_divergence_bps,min_free_collateral_ratio"),
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
        "max_position_notional_usd": config.max_position_notional_usd,
        "max_total_exposure_usd": config.max_total_exposure_usd,
        "max_portfolio_leverage": config.max_portfolio_leverage,
        "max_price_divergence_bps": config.max_price_divergence_bps,
        "min_free_collateral_ratio": config.min_free_collateral_ratio
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
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact(origin))
                .allow_methods([Method::GET, Method::PUT])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        );

    let address: SocketAddr = bind.parse().expect("API_BIND must be a valid socket address");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind API listener");
    axum::serve(listener, app).await.expect("API server failed");
}
