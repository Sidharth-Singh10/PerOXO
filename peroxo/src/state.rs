use crate::actors::{
    connection_manager::ConnectionManager,
    message_router::{MessageRouter, RouterMessage},
};
use axum::{Json, extract::State, response::IntoResponse};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::error;

pub struct AppState {
    pub connection_manager: Arc<ConnectionManager>,
    pub router_sender: mpsc::UnboundedSender<RouterMessage>,
}

impl AppState {
    pub fn new() -> Self {
        let (router, router_sender) = MessageRouter::new();
        let connection_manager = Arc::new(ConnectionManager::new(router_sender.clone()));

        // Spawn the message router
        tokio::spawn(router.run());

        Self {
            connection_manager,
            router_sender,
        }
    }
}

// Get list of online users
pub async fn get_online_users(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let (respond_to, response) = oneshot::channel();
    let msg = RouterMessage::GetOnlineUsers { respond_to };

    if state.router_sender.send(msg).is_err() {
        error!("Failed to communicate with message router");
        return Json(Vec::<String>::new());
    }

    match response.await {
        Ok(users) => Json(users),
        Err(_) => {
            error!("Failed to get response from message router");
            Json(Vec::<String>::new())
        }
    }
}
