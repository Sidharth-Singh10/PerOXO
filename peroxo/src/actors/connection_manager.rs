use crate::{
    actors::{message_router::RouterMessage, user_session::session::UserSession},
    metrics::Metrics,
    tenant::TenantUserId,
};
use axum::extract::ws::WebSocket;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct ConnectionManager {
    router_sender: mpsc::Sender<RouterMessage>,
    drain_tx: tokio::sync::broadcast::Sender<()>,
}

impl ConnectionManager {
    pub fn new(
        router_sender: mpsc::Sender<RouterMessage>,
        drain_tx: tokio::sync::broadcast::Sender<()>,
    ) -> Self {
        Self {
            router_sender,
            drain_tx,
        }
    }

    pub async fn handle_connection(&self, socket: WebSocket, tenant_user_id: TenantUserId) {
        info!("New connection attempt for user: {}", tenant_user_id);

        let drain_rx = self.drain_tx.subscribe();

        match UserSession::new(tenant_user_id.clone(), socket, self.router_sender.clone(), drain_rx)
            .await
        {
            Ok(session) => {
                info!("User session created for: {}", tenant_user_id);
                Metrics::websocket_connected();
                session.run().await;
            }
            Err(e) => {
                error!("Failed to create session for {}: {}", tenant_user_id, e);
            }
        }
    }
}
