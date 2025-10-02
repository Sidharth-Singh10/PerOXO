#[cfg(feature = "mongo_db")]
use mongodb::bson::doc;
use tokio::sync::{mpsc, oneshot};
#[cfg(feature = "persistence")]
use tonic::{Request, transport::Channel};
use tracing::{debug, error, info};

#[cfg(feature = "persistence")]
use tracing::warn;

#[cfg(feature = "persistence")]
use crate::actors::chat_service::{
    GetPaginatedMessagesRequest, WriteDmRequest, WriteDmResponse,
    chat_service_client::ChatServiceClient,
};
use crate::chat::ResponseDirectMessage;
#[cfg(feature = "mongo_db")]
use crate::mongo_db::config::MongoDbConfig;

#[derive(Debug)]
pub enum PersistenceMessage {
    PersistDirectMessage {
        sender_id: i32,
        receiver_id: i32,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    GetPaginatedMessages {
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    },
}

pub struct PersistenceActor {
    receiver: mpsc::UnboundedReceiver<PersistenceMessage>,
    #[cfg(feature = "persistence")]
    chat_service_client: ChatServiceClient<Channel>,
    #[cfg(feature = "mongo_db")]
    mango_db_client: mongodb::Client,
    #[cfg(feature = "mongo_db")]
    mongo_config: MongoDbConfig,
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
            }
        }

        info!("Persistence actor stopped");
    }

    async fn handle_persist_direct_message(
        &mut self,
        sender_id: i32,
        receiver_id: i32,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
    ) -> Result<(), String> {
        #[cfg(feature = "mongo_db")]
        {
            use crate::mongo_db::models::DirectMessageId;

            let message = crate::mongo_db::models::DirectMessage {
                id: DirectMessageId {
                    conversation_id: if sender_id < receiver_id {
                        format!("{}_{}", sender_id, receiver_id)
                    } else {
                        format!("{}_{}", receiver_id, sender_id)
                    },
                    message_id: message_id.to_string(),
                },

                sender_id,
                recipient_id: receiver_id,
                message_text: message_content.clone(),
                created_at: mongodb::bson::DateTime::from_millis(timestamp),
            };

            if let Err(e) = self
                .insert_message_in_mongo(&self.mango_db_client, message)
                .await
            {
                error!("Failed to persist message to MongoDB: {}", e);
                return Err(format!("MongoDB persistence failed: {}", e));
            }
            debug!(
                "Successfully persisted message to MongoDB from {} to {}",
                sender_id, receiver_id
            );
            Ok(())
        }

        #[cfg(feature = "persistence")]
        // Make the gRPC call with retry logic
        match self
            .write_dm_with_retry(
                sender_id,
                receiver_id,
                message_content,
                message_id,
                timestamp,
                3,
            )
            .await
        {
            Ok(response) => {
                let write_dm_response = response.into_inner();
                if write_dm_response.success {
                    debug!(
                        "Successfully persisted message from {} to {}",
                        sender_id, receiver_id
                    );
                    Ok(())
                } else {
                    error!(
                        "Failed to persist message: {}",
                        write_dm_response.error_message
                    );
                    Err(write_dm_response.error_message)
                }
            }
            Err(e) => {
                error!("gRPC call failed: {}", e);
                Err(format!("gRPC call failed: {}", e))
            }
        }
    }

    #[cfg(feature = "mongo_db")]
    async fn insert_message_in_mongo(
        &self,
        client: &mongodb::Client,
        message: crate::mongo_db::models::DirectMessage,
    ) -> Result<(), mongodb::error::Error> {
        let db = client.database(&self.mongo_config.database_name);
        let messages_col =
            db.collection::<crate::mongo_db::models::DirectMessage>("direct_messages");
        let conv_col =
            db.collection::<crate::mongo_db::models::UserConversation>("user_conversations");

        // Start a session for transaction
        let mut session = client.start_session().await?;
        session.start_transaction().await?;

        // Insert the message
        messages_col
            .insert_one(message.clone())
            .session(&mut session)
            .await?;

        // Update last_message for sender
        conv_col
        .update_one(
            doc! { "_id.user_id": message.sender_id, "_id.conversation_id": &message.id.conversation_id },
            doc! { "$set": { "last_message": message.created_at } },
        )
        .upsert(true)
        .session(&mut session)
        .await?;

        // Update last_message for recipient
        conv_col
        .update_one(
            doc! { "_id.user_id": message.recipient_id, "_id.conversation_id": &message.id.conversation_id },
            doc! { "$set": { "last_message": message.created_at } },
        )
        .upsert(true)
        .session(&mut session)
        .await?;

        session.commit_transaction().await?;

        Ok(())
    }

    #[cfg(feature = "persistence")]
    async fn write_dm_with_retry(
        &mut self,
        sender_id: i32,
        receiver_id: i32,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
        max_retries: u32,
    ) -> Result<tonic::Response<WriteDmResponse>, tonic::Status> {
        let mut attempts = 0;
        let mut last_error = None;

        // Maybe a Better Retry Logic
        while attempts <= max_retries {
            let request = Request::new(WriteDmRequest {
                sender_id,
                receiver_id,
                message: message_content.clone(),
                message_id: message_id.to_string(),
                timestamp,
            });

            #[cfg(feature = "persistence")]
            match self.chat_service_client.write_dm(request).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    attempts += 1;
                    last_error = Some(e);

                    if attempts <= max_retries {
                        let delay = std::time::Duration::from_millis(100 * attempts as u64);
                        warn!(
                            "gRPC call failed (attempt {}/{}), retrying in {:?}: {}",
                            attempts,
                            max_retries + 1,
                            delay,
                            last_error.as_ref().unwrap()
                        );
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    async fn handle_get_paginated_messages(
        &mut self,
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
    ) -> Result<PaginatedMessagesResponse, String> {
        #[cfg(feature = "mongo_db")]
        {
            return self
                .fetch_paginated_messages_from_mongo(conversation_id, message_id)
                .await;
        }

        #[cfg(feature = "persistence")]
        {
            let cursor_message_id = message_id.map(|id| id.to_string()).unwrap_or_default();

            let request = Request::new(GetPaginatedMessagesRequest {
                conversation_id,
                cursor_message_id,
            });

            match self
                .chat_service_client
                .get_paginated_messages(request)
                .await
            {
                Ok(response) => {
                    let get_paginated_response = response.into_inner();
                    if get_paginated_response.success {
                        let messages: Vec<ResponseDirectMessage> = get_paginated_response
                            .messages
                            .into_iter()
                            .map(|msg| ResponseDirectMessage {
                                conversation_id: msg.conversation_id,
                                message_id: uuid::Uuid::parse_str(&msg.message_id)
                                    .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                                sender_id: msg.sender_id,
                                recipient_id: msg.recipient_id,
                                message_text: msg.message_text,
                                created_at: msg.created_at,
                            })
                            .collect::<Vec<ResponseDirectMessage>>();

                        let next_cursor = if get_paginated_response.next_cursor.is_empty() {
                            None
                        } else {
                            Some(get_paginated_response.next_cursor)
                        };

                        let has_more = next_cursor.is_some();

                        debug!("Successfully fetched {} paginated messages", messages.len());
                        Ok(PaginatedMessagesResponse {
                            messages,
                            next_cursor,
                            has_more,
                        })
                    } else {
                        error!(
                            "Failed to fetch paginated messages: {}",
                            get_paginated_response.error_message
                        );
                        Err(get_paginated_response.error_message)
                    }
                }
                Err(e) => {
                    error!("gRPC call failed: {}", e);
                    Err(format!("gRPC call failed: {}", e))
                }
            }
        }
    }

    #[cfg(feature = "mongo_db")]
    async fn fetch_paginated_messages_from_mongo(
        &self,
        conversation_id: String,
        cursor: Option<uuid::Uuid>,
    ) -> Result<PaginatedMessagesResponse, String> {
        use crate::mongo_db::models::DirectMessage as MongoDirectMessage;
        use mongodb::bson::doc;

        let db = self
            .mango_db_client
            .database(&self.mongo_config.database_name);
        let messages_col = db.collection::<MongoDirectMessage>("direct_messages");

        // Build the query
        let mut filter = doc! { "_id.conversation_id": &conversation_id };

        if let Some(cursor_id) = cursor {
            filter.insert("_id.message_id", doc! { "$lt": cursor_id.to_string() });
        }

        // Fetch messages with limit
        let mut cursor = messages_col
            .find(filter)
            .sort(doc! { "_id.message_id": -1 })
            .limit(50)
            .await
            .map_err(|e| format!("MongoDB query failed: {}", e))?;

        let mut messages = Vec::new();

        use futures::stream::TryStreamExt;
        while let Some(doc) = cursor
            .try_next()
            .await
            .map_err(|e| format!("Failed to iterate cursor: {}", e))?
        {
            messages.push(ResponseDirectMessage {
                conversation_id: doc.id.conversation_id,
                message_id: uuid::Uuid::parse_str(&doc.id.message_id)
                    .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                sender_id: doc.sender_id,
                recipient_id: doc.recipient_id,
                message_text: doc.message_text,
                created_at: doc.created_at.timestamp_millis(),
            });
        }

        let next_cursor = messages.last().map(|msg| msg.message_id.to_string());
        let has_more = messages.len() == 50;

        debug!(
            "Successfully fetched {} paginated messages from MongoDB",
            messages.len()
        );

        Ok(PaginatedMessagesResponse {
            messages,
            next_cursor,
            has_more,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PaginatedMessagesResponse {
    pub messages: Vec<ResponseDirectMessage>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
