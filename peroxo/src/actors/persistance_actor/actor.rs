#[cfg(feature = "persistence")]
use tonic::transport::Channel;

#[cfg(feature = "persistence")]
use crate::chat_service_client::ChatServiceClient;
#[cfg(feature = "mongo_db")]
use crate::mongo_db::config::MongoDbConfig;
use std::sync::Arc;

pub struct PersistenceService {
    #[cfg(feature = "persistence")]
    pub chat_service_client: ChatServiceClient<Channel>,
    #[cfg(feature = "mongo_db")]
    pub mango_db_client: mongodb::Client,
    #[cfg(feature = "mongo_db")]
    pub mongo_config: MongoDbConfig,
    /// Bounds the number of in-flight gRPC persistence calls to prevent
    /// unbounded task spawns from overwhelming chat-service.
    pub semaphore: Arc<tokio::sync::Semaphore>,
}

const MAX_CONCURRENT_PERSISTENCE: usize = 256;

impl PersistenceService {
    pub fn new(
        #[cfg(feature = "persistence")] chat_service_client: ChatServiceClient<Channel>,
        #[cfg(feature = "mongo_db")] mango_db_client: mongodb::Client,
        #[cfg(feature = "mongo_db")] mongo_config: MongoDbConfig,
    ) -> Self {
        Self {
            #[cfg(feature = "persistence")]
            chat_service_client,
            #[cfg(feature = "mongo_db")]
            mango_db_client,
            #[cfg(feature = "mongo_db")]
            mongo_config,
            semaphore: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_PERSISTENCE)),
        }
    }
}
