use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    extract::Extension,
    http::{
        HeaderName, HeaderValue, Method,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    routing::{get, post},
};
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{error, info, warn};

mod db;
mod google_auth;
mod grpc;
mod grpc_auth;
mod handlers;
mod rate_limit;
mod redact;
mod tenant;
mod user_token;

/// Auth payloads are tiny; a larger body is abuse, not a legitimate request.
const MAX_REQUEST_BODY_BYTES: usize = 16 * 1024;

fn cors_layer() -> Result<CorsLayer, Box<dyn std::error::Error + Send + Sync>> {
    let origins_raw =
        env::var("ALLOWED_ORIGINS").unwrap_or_else(|_| "https://docs.mutref.tech".to_string());

    let mut allowed_origins = Vec::new();
    for origin in origins_raw
        .split(',')
        .map(str::trim)
        .filter(|o| !o.is_empty())
    {
        if origin == "*" {
            return Err("ALLOWED_ORIGINS must not contain a wildcard".into());
        }

        let value = origin
            .parse::<HeaderValue>()
            .map_err(|e| format!("invalid origin in ALLOWED_ORIGINS ({origin}): {e}"))?;
        allowed_origins.push(value);
    }

    if allowed_origins.is_empty() {
        return Err("ALLOWED_ORIGINS is set but contains no usable origins".into());
    }

    info!(
        origins = %origins_raw,
        count = allowed_origins.len(),
        "CORS allowed origins"
    );

    Ok(CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]))
}

fn security_headers() -> [(&'static str, &'static str); 6] {
    [
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
        (
            "content-security-policy",
            "default-src 'none'; frame-ancestors 'none'",
        ),
        (
            "strict-transport-security",
            "max-age=31536000; includeSubDomains",
        ),
        ("cache-control", "no-store"),
    ]
}

fn apply_security_headers(app: Router) -> Router {
    let mut app = app;
    for (name, value) in security_headers() {
        app = app.layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        ));
    }
    app
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .with_target(true)
        .with_line_number(true)
        .init();

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env file");
    let pool = PgPool::connect(&database_url).await?;
    info!("connected to postgres");

    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_client = redis::Client::open(redis_url.clone())?;
    // Never log a credential-bearing URL verbatim.
    info!(
        redis_url = %redact::redact_url_password(&redis_url),
        "redis client created"
    );

    // Multiplexed async connection: ConnectionManager reuses one connection
    // instead of opening a blocking connection per call.
    let redis_manager = redis::aio::ConnectionManager::new(redis_client).await?;
    let redis_arc = Arc::new(redis_manager);

    let grpc_redis = redis_arc.clone();
    let grpc_addr = env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50051".to_string())
        .parse()?;
    info!(grpc_addr = %grpc_addr, "starting gRPC server");

    // Resolve the shared secret before spawning: a missing secret must fail
    // startup, not leave an unauthenticated gRPC port running in the dark.
    let grpc_secret = crate::grpc_auth::require_shared_secret("auth-service")?;

    tokio::spawn(async move {
        if let Err(e) = grpc::start_grpc_server(grpc_addr, grpc_redis, grpc_secret).await {
            error!(%e, "gRPC server failed");
        }
    });

    let google_client_id = env::var("GOOGLE_CLIENT_ID").ok();
    match &google_client_id {
        Some(id) => info!(client_id = %id, "Google OAuth configured"),
        None => warn!("GOOGLE_CLIENT_ID not set — /generate-tenant will reject all requests"),
    }

    let app = apply_security_headers(
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/generate-tenant", get(handlers::generate_tenant_handler))
            .route(
                "/generate-user-token",
                post(handlers::generate_user_token_handler),
            )
            .route(
                "/verify-user-token",
                post(handlers::verify_user_token_handler),
            )
            .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
            .layer(cors_layer()?)
            .layer(Extension(pool))
            .layer(Extension(redis_arc)),
    );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3004").await?;
    info!("server listening on 0.0.0.0:3004");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
