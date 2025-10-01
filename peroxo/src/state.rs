use crate::actors::{
    connection_manager::ConnectionManager,
    message_router::{MessageRouter, RouterMessage},
};

#[cfg(feature = "persistence")]
use crate::actors::chat_service::chat_service_client::ChatServiceClient;
#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::actors::persistance_actor::PersistenceActor;

use std::sync::Arc;
use tokio::sync::mpsc;
pub struct PerOxoState {
    pub connection_manager: Arc<ConnectionManager>,
    pub router_sender: mpsc::UnboundedSender<RouterMessage>,
}

impl PerOxoState {
    async fn new(
        #[cfg(feature = "persistence")] chat_service_client: ChatServiceClient<Channel>,
        #[cfg(feature = "mongo_db")] mango_db_client: mongodb::Client,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        let (persistence_actor, persistence_sender) = PersistenceActor::new(
            #[cfg(feature = "persistence")]
            chat_service_client,
            #[cfg(feature = "mongo_db")]
            mango_db_client,
        )
        .await?;

        let (router, router_sender) = MessageRouter::new(
            #[cfg(any(feature = "mongo_db", feature = "persistence"))]
            persistence_sender,
        );
        let connection_manager = Arc::new(ConnectionManager::new(router_sender.clone()));

        // Spawn actors
        tokio::spawn(router.run());
        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        tokio::spawn(persistence_actor.run());

        Ok(Self {
            connection_manager,
            router_sender,
        })
    }
}

pub struct PerOxoStateBuilder {
    #[cfg(feature = "persistence")]
    connection_url: Option<String>,
    #[cfg(feature = "mongo_db")]
    mango_db_url: Option<String>,
}

impl PerOxoStateBuilder {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "persistence")]
            connection_url: None,
            #[cfg(feature = "mongo_db")]
            mango_db_url: None,
        }
    }

    #[cfg(feature = "persistence")]
    pub fn with_persistence_connection_url(mut self, url: impl Into<String>) -> Self {
        self.connection_url = Some(url.into());
        self
    }

    #[cfg(feature = "mongo_db")]
    pub fn with_mango_db_connection_url(mut self, url: impl Into<String>) -> Self {
        self.mango_db_url = Some(url.into());
        self
    }

    pub async fn build(self) -> Result<PerOxoState, Box<dyn std::error::Error>> {
        #[cfg(feature = "persistence")]
        let chat_service_client = if let Some(url) = self.connection_url {
            connect_chat_service_client(url).await?
        } else {
            return Err("connection_url required when persistence is enabled".into());
        };

        #[cfg(feature = "mongo_db")]
        let mango_db_client = if let Some(url) = self.mango_db_url {
            use crate::connections::connect_mongo_db_client;

            connect_mongo_db_client(url).await?
        } else {
            return Err("mango_db_url required when mangodb is enabled".into());
        };

        PerOxoState::new(
            #[cfg(feature = "persistence")]
            chat_service_client,
            #[cfg(feature = "mongo_db")]
            mango_db_client,
        )
        .await
    }
}
