use crate::actors::{
    connection_manager::ConnectionManager,
    message_router::{MessageRouter, RouterMessage},
};

#[cfg(feature = "persistence")]
use crate::actors::chat_service::chat_service_client::ChatServiceClient;
#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::actors::persistance_actor::PersistenceActor;
#[cfg(feature = "mongo_db")]
use crate::mongo_db::config::MongoDbConfig;

use std::sync::Arc;
use tokio::sync::mpsc;
#[cfg(feature = "persistence")]
use tonic::transport::Channel;
pub struct PerOxoState {
    pub connection_manager: Arc<ConnectionManager>,
    pub router_sender: mpsc::UnboundedSender<RouterMessage>,
    pub auth_client: crate::auth_service_client::AuthServiceClient<Channel>,
}

impl PerOxoState {
    async fn new(
        #[cfg(feature = "persistence")] chat_service_client: ChatServiceClient<Channel>,
        #[cfg(feature = "mongo_db")] mango_db_client: mongodb::Client,
        #[cfg(feature = "mongo_db")] mongo_config: MongoDbConfig,
        auth_client: crate::auth_service_client::AuthServiceClient<Channel>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        let (persistence_actor, persistence_sender) = PersistenceActor::new(
            #[cfg(feature = "persistence")]
            chat_service_client,
            #[cfg(feature = "mongo_db")]
            mango_db_client,
            #[cfg(feature = "mongo_db")]
            mongo_config,
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
            auth_client,
        })
    }
}

pub struct PerOxoStateBuilder {
    #[cfg(feature = "persistence")]
    connection_url: Option<String>,
    #[cfg(feature = "mongo_db")]
    mongo_config: Option<MongoDbConfig>,
    auth_url: Option<String>,
}

impl Default for PerOxoStateBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PerOxoStateBuilder {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "persistence")]
            connection_url: None,
            #[cfg(feature = "mongo_db")]
            mongo_config: None,
            auth_url: None,
        }
    }

    #[cfg(feature = "persistence")]
    pub fn with_persistence_connection_url(mut self, url: impl Into<String>) -> Self {
        self.connection_url = Some(url.into());
        self
    }

    #[cfg(feature = "mongo_db")]
    pub fn with_mongo_config(mut self, config: MongoDbConfig) -> Self {
        self.mongo_config = Some(config);
        self
    }

    #[cfg(feature = "mongo_db")]
    pub fn with_mango_db_connection_url(mut self, url: impl Into<String>) -> Self {
        self.mongo_config = Some(MongoDbConfig::new(url));
        self
    }

    pub fn with_auth_url(mut self, url: impl Into<String>) -> Self {
        self.auth_url = Some(url.into());
        self
    }

    pub async fn build(self) -> Result<PerOxoState, Box<dyn std::error::Error>> {
        #[cfg(feature = "persistence")]
        let chat_service_client = if let Some(url) = self.connection_url {
            use crate::connections::connect_chat_service_client;
            connect_chat_service_client(url).await?
        } else {
            return Err("connection_url required when persistence is enabled".into());
        };

        #[cfg(feature = "mongo_db")]
        let (mango_db_client, mongo_config) = if let Some(config) = self.mongo_config {
            use crate::connections::connect_mongo_db_client;
            let client = connect_mongo_db_client(&config.connection_url).await?;
            (client, config)
        } else {
            return Err("mongo_config required when mongo_db is enabled".into());
        };

        let auth_service_client = if let Some(url) = self.auth_url {
            use crate::connections::connect_auth_service_client;
            connect_auth_service_client(url).await?
        } else {
            return Err("auth_url is required".into());
        };

        PerOxoState::new(
            #[cfg(feature = "persistence")]
            chat_service_client,
            #[cfg(feature = "mongo_db")]
            mango_db_client,
            #[cfg(feature = "mongo_db")]
            mongo_config,
            auth_service_client,
        )
        .await
    }
}
