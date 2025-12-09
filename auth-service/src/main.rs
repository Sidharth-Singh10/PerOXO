use axum::{
    Router,
    extract::Extension,
    routing::{get, post},
};
use sqlx::PgPool;

use crate::handlers::{generate_user_token_handler, verify_user_token_handler};

mod db;
mod handlers;
mod tenant;
mod user_token;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env file");

    let pool = PgPool::connect(&database_url).await?;

    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_client = redis::Client::open(redis_url)?;

    let app = Router::new()
        .route("/generate-tenant", get(handlers::generate_tenant_handler))
        .route("/generate-user-token", post(generate_user_token_handler))
        .route("/verify-user-token", post(verify_user_token_handler))
        .layer(Extension((pool, redis_client)));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3004").await.unwrap();

    axum::serve(listener, app).await.unwrap();

    Ok(())
}
