use axum::{
    body::Body,
    extract::{Extension, State},
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
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

#[derive(Clone, Debug)]
struct AuthenticatedUser {
    id: String,
    email: Option<String>,
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

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "fmt-api",
        execution_enabled: false,
    })
}

async fn me(Extension(user): Extension<AuthenticatedUser>) -> Json<MeResponse> {
    Json(MeResponse {
        user_id: user.id,
        email: user.email,
    })
}

async fn authenticate(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let Some(auth_header) = request.headers().get(header::AUTHORIZATION).cloned() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(auth_value) = auth_header.to_str() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !auth_value.starts_with("Bearer ") || auth_value.len() <= 7 {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let response = match state
        .http
        .get(format!("{}/auth/v1/user", state.supabase_url.trim_end_matches('/')))
        .header("apikey", &state.supabase_anon_key)
        .header(header::AUTHORIZATION, auth_value)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };

    if response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !response.status().is_success() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    let user = match response.json::<SupabaseUser>().await {
        Ok(user) if !user.id.is_empty() => user,
        _ => return StatusCode::UNAUTHORIZED.into_response(),
    };

    request.extensions_mut().insert(AuthenticatedUser {
        id: user.id,
        email: user.email,
    });
    next.run(request).await
}

#[tokio::main]
async fn main() {
    let supabase_url = env::var("SUPABASE_URL")
        .expect("SUPABASE_URL must be configured");
    let supabase_anon_key = env::var("SUPABASE_ANON_KEY")
        .expect("SUPABASE_ANON_KEY must be configured");
    let bind = env::var("API_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let allowed_origin = env::var("CORS_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    let origin = HeaderValue::from_str(&allowed_origin)
        .expect("CORS_ORIGIN must be a valid origin URL");
    let state = AppState {
        http: Client::new(),
        supabase_url,
        supabase_anon_key,
    };

    let protected = Router::new()
        .route("/v1/me", get(me))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate));

    let app = Router::new()
        .route("/health", get(health))
        .merge(protected)
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact(origin))
                .allow_methods([Method::GET])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        )
        .with_state(state);

    let address: SocketAddr = bind.parse().expect("API_BIND must be a valid socket address");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind API listener");
    axum::serve(listener, app).await.expect("API server failed");
}
