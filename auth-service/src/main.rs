use std::env;

use axum::{
    Router,
    extract::Extension,
    http::{HeaderValue, Method, header::{AUTHORIZATION, CONTENT_TYPE}},
    routing::{get, post},
};
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

mod db;
mod google_auth;
mod grpc;
mod handlers;
mod rate_limit;
mod tenant;
mod user_token;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("error".parse().unwrap()), // deepest log level
        )
        .with_target(true)
        .with_line_number(true)
        .init();

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env file");
    let pool = PgPool::connect(&database_url).await?;

    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_client = redis::Client::open(redis_url)?;

    let grpc_redis = redis_client.clone();
    let grpc_addr = env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50051".to_string())
        .parse()?;

    tokio::spawn(async move {
        if let Err(e) = grpc::start_grpc_server(grpc_addr, grpc_redis).await {
            error!(%e, "gRPC server failed");
        }
    });

    let allowed_origins: Vec<HeaderValue> = env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "https://docs.mutref.tech".to_string())
        .split(',')
        .map(|s| s.trim().parse::<HeaderValue>().expect("invalid origin in ALLOWED_ORIGINS"))
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    let app = Router::new()
        .route("/generate-tenant", get(handlers::generate_tenant_handler))
        .route(
            "/generate-user-token",
            post(handlers::generate_user_token_handler),
        )
        .route(
            "/verify-user-token",
            post(handlers::verify_user_token_handler),
        )
        .layer(cors)
        .layer(Extension(pool))
        .layer(Extension(redis_client));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3004").await.unwrap();
    info!("server listening on 0.0.0.0:3004");
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
