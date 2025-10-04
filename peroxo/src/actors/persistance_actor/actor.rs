use tokio::sync::mpsc;
use tracing::info;

use super::messages::PersistenceMessage;

#[cfg(feature = "persistence")]
use crate::actors::chat_service::chat_service_client::ChatServiceClient;
#[cfg(feature = "persistence")]
use tonic::transport::Channel;

#[cfg(feature = "mongo_db")]
use crate::mongo_db::config::MongoDbConfig;

pub struct PersistenceActor {
    pub receiver: mpsc::UnboundedReceiver<PersistenceMessage>,
    #[cfg(feature = "persistence")]
    pub chat_service_client: ChatServiceClient<Channel>,
    #[cfg(feature = "mongo_db")]
    pub mango_db_client: mongodb::Client,
    #[cfg(feature = "mongo_db")]
    pub mongo_config: MongoDbConfig,
}

impl PersistenceActor {
    pub async fn new(
        #[cfg(feature = "persistence")] chat_service_client: ChatServiceClient<Channel>,
        #[cfg(feature = "mongo_db")] mango_db_client: mongodb::Client,
        #[cfg(feature = "mongo_db")] mongo_config: MongoDbConfig,
    ) -> Result<(Self, mpsc::UnboundedSender<PersistenceMessage>), Box<dyn std::error::Error>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let actor = Self {
            receiver,
            #[cfg(feature = "persistence")]
            chat_service_client,
            #[cfg(feature = "mongo_db")]
            mango_db_client,
            #[cfg(feature = "mongo_db")]
            mongo_config,
        };

        Ok((actor, sender))
    }

    pub async fn run(mut self) {
        info!("Persistence actor started");

        while let Some(message) = self.receiver.recv().await {
            match message {
                PersistenceMessage::PersistDirectMessage {
                    sender_id,
                    receiver_id,
                    message_content,
                    message_id,
                    timestamp,
                    respond_to,
                } => {
                    let result = self
                        .handle_persist_direct_message(
                            sender_id,
                            receiver_id,
                            message_content,
                            message_id,
                            timestamp,
                        )
                        .await;

                    let _ = respond_to.send(result);
                }
                PersistenceMessage::GetPaginatedMessages {
                    message_id,
                    conversation_id,
                    respond_to,
                } => {
                    let result = self
                        .handle_get_paginated_messages(message_id, conversation_id)
                        .await;
                    let _ = respond_to.send(result);
                }
                PersistenceMessage::PersistRoomMessage {
                    room_id: _,
                    sender_id: _,
                    message_content: _,
                    message_id: _,
                    timestamp: _,
                    respond_to: _,
                } => {
                    info!("PersistRoomMessage is not yet implemented");
                }
            }
        }

        info!("Persistence actor stopped");
    }
}
