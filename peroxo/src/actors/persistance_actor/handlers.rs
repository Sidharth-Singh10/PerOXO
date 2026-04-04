use tracing::{debug, error};

use super::actor::PersistenceService;

#[cfg(feature = "persistence")]
use crate::{
    WriteDmRequest, WriteDmResponse, WriteRoomMessageRequest, WriteRoomMessageResponse, chat::{PaginatedMessagesResponse, ResponseDirectMessage}, tenant::TenantUserId
};

impl PersistenceService {
    #[allow(unreachable_code, unused_variables)]
    pub async fn handle_persist_direct_message(
        &self,
        conversation_id: String,
        tenant_sender_id: TenantUserId,
        tenant_receiver_id: TenantUserId,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
    ) -> Result<(), String> {
        #[cfg(feature = "mongo_db")]
        {
            use crate::metrics::Metrics;
            use crate::mongo_db::models::DirectMessageId;

            let sender_id = tenant_sender_id.user_id.clone();
            let receiver_id = tenant_receiver_id.user_id.clone();

            let message = crate::mongo_db::models::DirectMessage {
                id: DirectMessageId {
                    conversation_id: if sender_id < receiver_id {
                        format!("{}_{}", sender_id, receiver_id)
                    } else {
                        format!("{}_{}", receiver_id, sender_id)
                    },
                    message_id: message_id.to_string(),
                },

                sender_id: sender_id.parse::<i32>().unwrap_or_default(),
                recipient_id: receiver_id.parse::<i32>().unwrap_or_default(),
                message_text: message_content.clone(),
                created_at: mongodb::bson::DateTime::from_millis(timestamp),
            };

            let start = std::time::Instant::now();

            if let Err(e) = self
                .insert_message_in_mongo(&self.mango_db_client, message)
                .await
            {
                error!("Failed to persist message to MongoDB: {}", e);
                return Err(format!("MongoDB persistence failed: {}", e));
            }

            Metrics::observe_db_query("mongo_db_write", start.elapsed());
            debug!(
                "Successfully persisted message to MongoDB from {} to {}",
                sender_id, receiver_id
            );
            return Ok(());
        }

        #[cfg(feature = "persistence")]
        {
            use crate::WriteDmRequest;

            

            let start = std::time::Instant::now();
            let request = WriteDmRequest {
                project_id: tenant_sender_id.project_id.clone(),
                conversation_id: conversation_id.clone(),
                sender_id: tenant_sender_id.user_id.clone(),
                receiver_id: tenant_receiver_id.user_id.clone(),
                message: message_content,
                message_id: message_id.to_string(),
                timestamp,
            };

            match self.write_dm_with_retry(request, 3).await
            {
                Ok(response) => {
                    let write_dm_response = response.into_inner();
                    if write_dm_response.success {
                        use crate::metrics::Metrics;

                        debug!(
                            "Successfully persisted message from {} to {}",
                            tenant_sender_id, tenant_receiver_id
                        );
                        Metrics::observe_db_query("grpc_write_dm", start.elapsed());
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
    }

    #[cfg(feature = "mongo_db")]
    pub async fn insert_message_in_mongo(
        &self,
        client: &mongodb::Client,
        message: crate::mongo_db::models::DirectMessage,
    ) -> Result<(), mongodb::error::Error> {
        use mongodb::bson::doc;

        let db = client.database(&self.mongo_config.database_name);
        let messages_col =
            db.collection::<crate::mongo_db::models::DirectMessage>("direct_messages");
        let conv_col =
            db.collection::<crate::mongo_db::models::UserConversation>("user_conversations");

        let mut session = client.start_session().await?;
        session.start_transaction().await?;

        messages_col
            .insert_one(message.clone())
            .session(&mut session)
            .await?;

        conv_col
        .update_one(
            doc! { "_id.user_id": message.sender_id, "_id.conversation_id": &message.id.conversation_id },
            doc! { "$set": { "last_message": message.created_at } },
        )
        .upsert(true)
        .session(&mut session)
        .await?;

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
    pub async fn write_dm_with_retry(
        &self,
        request: WriteDmRequest,
        max_retries: u32,
    ) -> Result<tonic::Response<WriteDmResponse>, tonic::Status> {
        let mut client = self.chat_service_client.clone();
        let mut attempts = 0;
        let mut last_error = None;

        while attempts <= max_retries {
            use tonic::Request;

            match client.write_dm(Request::new(request.clone())).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    attempts += 1;
                    last_error = Some(e);

                    if attempts <= max_retries {
                        use tracing::warn;

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

    #[allow(unreachable_code, unused_variables)]
    pub async fn handle_get_paginated_messages(
        &self,
        project_id: String,
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
            use tonic::Request;

            use crate::GetPaginatedMessagesRequest;


            let mut client = self.chat_service_client.clone();
            let cursor_message_id = message_id.map(|id| id.to_string()).unwrap_or_default();

            let request = Request::new(GetPaginatedMessagesRequest {
                project_id,
                conversation_id,
                cursor_message_id,
            });

            match client.get_paginated_messages(request).await {
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

        let mut filter = doc! { "_id.conversation_id": &conversation_id };

        if let Some(cursor_id) = cursor {
            filter.insert("_id.message_id", doc! { "$lt": cursor_id.to_string() });
        }

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
                sender_id: doc.sender_id.to_string(),
                recipient_id: doc.recipient_id.to_string(),
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

    #[cfg(feature = "persistence")]
    pub async fn handle_persist_room_message(
        &self,
        room_id: String,
        sender_id: TenantUserId,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
    ) -> Result<(), String> {
        use crate::WriteRoomMessageRequest;


        let request = WriteRoomMessageRequest {
            project_id: sender_id.project_id.clone(),
            room_id: room_id.clone(),
            sender_id: sender_id.user_id.clone(),
            content: message_content,
            message_id: message_id.to_string(),
            timestamp,
        };

        match self.write_room_message_with_retry(request, 3).await
        {
            Ok(response) => {
                let write_room_response = response.into_inner();
                if write_room_response.success {
                    debug!(
                        "Successfully persisted room message from {} in room {}",
                        sender_id, room_id
                    );
                    Ok(())
                } else {
                    error!(
                        "Failed to persist room message: {}",
                        write_room_response.error_message
                    );
                    Err(write_room_response.error_message)
                }
            }
            Err(e) => {
                error!("gRPC call failed: {}", e);
                Err(format!("gRPC call failed: {}", e))
            }
        }
    }

    #[cfg(feature = "persistence")]
    pub async fn write_room_message_with_retry(
        &self,
        request: WriteRoomMessageRequest,
        max_retries: u32,
    ) -> Result<tonic::Response<WriteRoomMessageResponse>, tonic::Status>
    {
        let mut client = self.chat_service_client.clone();
        let mut attempts = 0;
        let mut last_error = None;

        while attempts <= max_retries {
            use tonic::Request;

            match client.write_room_message(Request::new(request.clone())).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    attempts += 1;
                    last_error = Some(e);

                    if attempts <= max_retries {
                        use tracing::warn;

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

    #[cfg(feature = "persistence")]
    pub async fn handle_sync_messages(
        &self,
        project_id: String,
        conversation_id: String,
        message_id: uuid::Uuid,
    ) -> Result<Vec<crate::chat::ResponseDirectMessage>, String> {
        use tonic::Request;

        use crate::SyncMessagesRequest;

        let mut client = self.chat_service_client.clone();

        let request = Request::new(SyncMessagesRequest {
            project_id,
            conversation_id: conversation_id.clone(),
            last_message_id: message_id.to_string(),
        });

        match client.sync_messages(request).await {
            Ok(response) => {
                let sync_response = response.into_inner();
                if sync_response.success {
                    let messages: Vec<crate::chat::ResponseDirectMessage> = sync_response
                        .messages
                        .into_iter()
                        .map(|msg| crate::chat::ResponseDirectMessage {
                            conversation_id: msg.conversation_id,
                            message_id: uuid::Uuid::parse_str(&msg.message_id)
                                .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                            sender_id: msg.sender_id,
                            recipient_id: msg.recipient_id,
                            message_text: msg.message_text,
                            created_at: msg.created_at,
                        })
                        .collect();

                    debug!(
                        "Successfully synced {} messages for conversation {}",
                        messages.len(),
                        conversation_id
                    );
                    Ok(messages)
                } else {
                    error!("Failed to sync messages: {}", sync_response.error_message);
                    Err(sync_response.error_message)
                }
            }
            Err(e) => {
                error!("gRPC call failed: {}", e);
                Err(format!("gRPC call failed: {}", e))
            }
        }
    }
}
