use per_oxo::{peroxo_route, state::PerOxoStateBuilder};
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // let state = MongoDbConfig::new("REMOVED_SECRET").with_database_name("affinity");
    let chat_service_addr = std::env::var("CHAT_SERVICE_ADDR").unwrap();
    let state = match PerOxoStateBuilder::new()
        // .with_mongo_config(state)
        .with_persistence_connection_url(chat_service_addr)
        .build()
        .await
    {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("Failed to build PerOxoState: {:?}", e);
            return;
        }
    };

    let app = peroxo_route(Arc::new(state));

    let addr = std::env::var("PER_OXO_SERVICE_ADDR").unwrap();
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    tracing::info!("Per-OXO service listening on {}", addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
