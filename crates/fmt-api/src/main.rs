use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{env, net::SocketAddr, str::FromStr};
use tower_http::cors::{AllowOrigin, CorsLayer};

#[derive(Clone)]
struct AppState {
    http: Client,
    supabase_url: String,
    supabase_anon_key: String,
}

#[derive(Debug, Deserialize)]
struct SupabaseUser {
    id: String,
    email: Option<String>,
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

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, StatusCode> {
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

    Ok(Json(MeResponse {
        user_id: user.id,
        email: user.email,
    }))
}

#[tokio::main]
async fn main() {
    let supabase_url =
        env::var("SUPABASE_URL").expect("SUPABASE_URL must be configured");
    let supabase_anon_key =
        env::var("SUPABASE_ANON_KEY").expect("SUPABASE_ANON_KEY must be configured");
    let bind = env::var("API_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let allowed_origin =
        env::var("CORS_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let origin = HeaderValue::from_str(&allowed_origin)
        .expect("CORS_ORIGIN must be a valid origin URL");
    let state = AppState {
        http: Client::new(),
        supabase_url,
        supabase_anon_key,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/me", get(me))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact(origin))
                .allow_methods([Method::GET])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        );

    let address: SocketAddr = bind.parse().expect("API_BIND must be a valid socket address");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind API listener");
    axum::serve(listener, app).await.expect("API server failed");
}
