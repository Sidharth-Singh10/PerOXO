use crate::{
    config::Config,
    peroxo_route,
    state::{PerOxoState, PerOxoStateBuilder},
};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;

pub async fn build_state(config: &Config) -> Result<PerOxoState, Box<dyn std::error::Error>> {
    PerOxoStateBuilder::new()
        .with_persistence_connection_url(&config.chat_service_addr)
        .with_auth_url(&config.auth_service_addr)
        .build()
        .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
        tracing::info!("Ctrl+C received");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
        tracing::info!("SIGTERM received");
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

pub async fn run(
    config: &Config,
    state: Arc<PerOxoState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = peroxo_route(state.clone());
    let listener = tokio::net::TcpListener::bind(&config.service_addr).await?;
    tracing::info!("Per-OXO service listening on {}", config.service_addr);

    let drain_tx = state.drain_tx.clone();

    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        tracing::info!("Shutting down, draining WebSocket sessions");
        let _ = drain_tx.send(());
    });

    // Belt and braces: never let a stuck client block shutdown forever.
    match tokio::time::timeout(Duration::from_secs(15), server).await {
        Ok(result) => result?,
        Err(_) => {
            tracing::warn!("Graceful shutdown timed out after 15s, forcing exit");
        }
    }

    Ok(())
}
