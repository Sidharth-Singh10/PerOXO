use axum::{Router, extract::Extension, routing::get};
use sqlx::PgPool;

use crate::handlers::generate_tenant_handler;

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

    let app = Router::new()
        .route("/generate-tenant", get(generate_tenant_handler))
        .layer(Extension(pool));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3004").await.unwrap();

    axum::serve(listener, app).await.unwrap();

    Ok(())
}
