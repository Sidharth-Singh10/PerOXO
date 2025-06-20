use crate::actors::{message_router::RouterMessage, user_session::UserSession};
use axum::extract::ws::WebSocket;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct ConnectionManager {
    router_sender: mpsc::UnboundedSender<RouterMessage>,
}

impl ConnectionManager {
    pub fn new(router_sender: mpsc::UnboundedSender<RouterMessage>) -> Self {
        Self { router_sender }
    }

    pub async fn handle_connection(&self, socket: WebSocket, username: String) {
        info!("New connection attempt for user: {}", username);

        match UserSession::new(username.clone(), socket, self.router_sender.clone()).await {
            Ok(session) => {
                info!("User session created for: {}", username);
                session.run().await;
            }
            Err(e) => {
                error!("Failed to create session for {}: {}", username, e);
            }
        }
    }
}
