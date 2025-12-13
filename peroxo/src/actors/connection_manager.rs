use crate::{
    UserToken,
    actors::{message_router::RouterMessage, user_session::session::UserSession},
    metrics::Metrics,
};
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

    pub async fn handle_connection(&self, socket: WebSocket, user_token: UserToken) {
        let user_id = user_token.user_id.parse::<i32>().unwrap();
        info!("New connection attempt for user: {}", user_id);

        match UserSession::new(user_token, socket, self.router_sender.clone()).await {
            Ok(session) => {
                info!("User session created for: {}", user_id);
                Metrics::websocket_connected();
                session.run().await;
            }
            Err(e) => {
                error!("Failed to create session for {}: {}", user_id, e);
            }
        }
    }
}
