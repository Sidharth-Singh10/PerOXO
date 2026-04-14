use std::{net::SocketAddr, sync::Arc};

use crate::{
    config::Config,
    peroxo_route,
    state::{PerOxoState, PerOxoStateBuilder},
};

pub async fn build_state(config: &Config) -> Result<PerOxoState, Box<dyn std::error::Error>> {
    PerOxoStateBuilder::new()
        .with_persistence_connection_url(&config.chat_service_addr)
        .with_auth_url(&config.auth_service_addr)
        .build()
        .await
}

pub async fn run(
    config: &Config,
    state: Arc<PerOxoState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = peroxo_route(state);
    let listener = tokio::net::TcpListener::bind(&config.service_addr).await?;
    tracing::info!("Per-OXO service listening on {}", config.service_addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
