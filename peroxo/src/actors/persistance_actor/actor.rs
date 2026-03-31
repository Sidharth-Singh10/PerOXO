#[cfg(feature = "persistence")]
use crate::actors::chat_service::chat_service_client::ChatServiceClient;
#[cfg(feature = "persistence")]
use tonic::transport::Channel;

#[cfg(feature = "mongo_db")]
use crate::mongo_db::config::MongoDbConfig;

pub struct PersistenceService {
    #[cfg(feature = "persistence")]
    pub chat_service_client: ChatServiceClient<Channel>,
    #[cfg(feature = "mongo_db")]
    pub mango_db_client: mongodb::Client,
    #[cfg(feature = "mongo_db")]
    pub mongo_config: MongoDbConfig,
}

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
        }
    }
}
