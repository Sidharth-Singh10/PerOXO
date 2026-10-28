use crate::actors::{
    connection_manager::ConnectionManager,
    message_router::{MessageRouter, RouterMessage},
};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::grpc_auth::AuthClient;
#[cfg(feature = "persistence")]
use crate::grpc_auth::ChatClient;

#[cfg(feature = "mongo_db")]
use crate::mongo_db::config::MongoDbConfig;

#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::actors::persistance_actor::PersistenceService;

pub struct PerOxoState {
    pub connection_manager: Arc<ConnectionManager>,
    pub router_sender: mpsc::Sender<RouterMessage>,
    pub auth_client: AuthClient,
    /// Broadcast that signals active WebSocket sessions to close during
    /// graceful shutdown.
    pub drain_tx: tokio::sync::broadcast::Sender<()>,
    #[cfg(feature = "persistence")]
    pub chat_client: ChatClient,
}

impl PerOxoState {
    async fn new(
        #[cfg(feature = "persistence")] chat_service_client: ChatClient,
        #[cfg(any(feature = "mongo_db", feature = "persistence"))] persistence: Arc<
            PersistenceService,
        >,
        auth_client: AuthClient,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (router, router_sender) = MessageRouter::new(
            #[cfg(any(feature = "mongo_db", feature = "persistence"))]
            persistence,
        );

        // Supervisor: if the router task panics, the in-process user registry
        // (the only authoritative presence/routing state) is unrecoverable.
        // Log it, record a metric, and exit so the container restarts with a
        // fresh instance — without this, a single panic silently kills all
        // routing while sessions keep queueing into a dead mailbox.
        tokio::spawn(async move {
            if let Err(join_error) = tokio::spawn(router.run()).await {
                if join_error.is_panic() {
                    crate::metrics::Metrics::router_panicked();
                    tracing::error!("MessageRouter panicked: {}", join_error);
                } else {
                    tracing::error!("MessageRouter task cancelled: {}", join_error);
                }
                std::process::exit(1);
            }
        });

        let (drain_tx, _) = tokio::sync::broadcast::channel(1);
        let connection_manager = Arc::new(ConnectionManager::new(
            router_sender.clone(),
            drain_tx.clone(),
        ));

        Ok(Self {
            connection_manager,
            router_sender,
            auth_client,
            drain_tx,
            #[cfg(feature = "persistence")]
            chat_client: chat_service_client,
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

        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        let persistence = Arc::new(PersistenceService::new(
            #[cfg(feature = "persistence")]
            chat_service_client.clone(),
            #[cfg(feature = "mongo_db")]
            mango_db_client,
            #[cfg(feature = "mongo_db")]
            mongo_config,
        ));

        PerOxoState::new(
            #[cfg(feature = "persistence")]
            chat_service_client,
            #[cfg(any(feature = "mongo_db", feature = "persistence"))]
            persistence,
            auth_service_client,
        )
        .await
    }
}
