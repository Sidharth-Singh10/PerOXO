use crate::actors::{
    connection_manager::ConnectionManager,
    message_router::{MessageRouter, RouterMessage},
};

#[cfg(feature = "persistence")]
use crate::actors::{
    chat_service::chat_service_client::ChatServiceClient, persistance_actor::PersistenceActor,
};

use std::sync::Arc;
use tokio::sync::mpsc;
pub struct AppState {
    pub connection_manager: Arc<ConnectionManager>,
    pub router_sender: mpsc::UnboundedSender<RouterMessage>,
}

impl AppState {
    async fn new(
        #[cfg(feature = "persistence")] chat_service_client: ChatServiceClient<Channel>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(feature = "persistence")]
        let (persistence_actor, persistence_sender) =
            PersistenceActor::new(chat_service_client).await?;

        let (router, router_sender) = MessageRouter::new(
            #[cfg(feature = "persistence")]
            persistence_sender,
        );
        let connection_manager = Arc::new(ConnectionManager::new(router_sender.clone()));

        // Spawn actors
        tokio::spawn(router.run());
        #[cfg(feature = "persistence")]
        tokio::spawn(persistence_actor.run());

        Ok(Self {
            connection_manager,
            router_sender,
        })
    }
}

pub struct AppStateBuilder {
    #[cfg(feature = "persistence")]
    connection_url: Option<String>,
}

impl AppStateBuilder {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "persistence")]
            connection_url: None,
        }
    }

    #[cfg(feature = "persistence")]
    pub fn with_persistence_connection_url(mut self, url: String) -> Self {
        self.connection_url = Some(url);
        self
    }

    pub async fn build(self) -> Result<AppState, Box<dyn std::error::Error>> {
        #[cfg(feature = "persistence")]
        let chat_service_client = if let Some(url) = self.connection_url {
            connect_chat_service_client(url).await?
        } else {
            return Err("connection_url required when persistence is enabled".into());
        };

        AppState::new(
            #[cfg(feature = "persistence")]
            chat_service_client,
        )
        .await
    }
}
