use crate::actors::{
    chat_service::chat_service_client::ChatServiceClient,
    connection_manager::ConnectionManager,
    message_router::{MessageRouter, RouterMessage},
    persistance_actor::PersistenceActor,
};
use axum::{Json, extract::State, response::IntoResponse};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tonic::transport::Channel;
use tracing::error;

pub struct AppState {
    pub connection_manager: Arc<ConnectionManager>,
    pub router_sender: mpsc::UnboundedSender<RouterMessage>,
}

impl AppState {
    pub async fn new(
        chat_service_client: ChatServiceClient<Channel>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (persistence_actor, persistence_sender) =
            PersistenceActor::new(chat_service_client).await?;
        let (router, router_sender) = MessageRouter::new(persistence_sender);
        let connection_manager = Arc::new(ConnectionManager::new(router_sender.clone()));

        // Spawn actors
        tokio::spawn(router.run());
        tokio::spawn(persistence_actor.run());

        Ok(Self {
            connection_manager,
            router_sender,
        })
    }
}

// Get list of online users
pub async fn get_online_users(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let (respond_to, response) = oneshot::channel();
    let msg = RouterMessage::GetOnlineUsers { respond_to };

    if state.router_sender.send(msg).is_err() {
        error!("Failed to communicate with message router");
        return Json(Vec::<i32>::new());
    }

    match response.await {
        Ok(users) => Json(users),
        Err(_) => {
            error!("Failed to get response from message router");
            Json(Vec::<i32>::new())
        }
    }
}
